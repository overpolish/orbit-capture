// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! The platform-neutral cursor recording stream.
//!
//! Native adapters only translate their mouse events and current cursor into
//! [`RawCursorEvent`]. Timing, pause removal, throttling and the file format
//! live here so macOS and Windows produce the same sidecar.

#[cfg(target_os = "macos")]
#[path = "cursor/platform_macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "cursor/platform_unsupported.rs"]
mod platform;

mod format;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(any(test, target_os = "macos"))]
pub(crate) use self::format::CursorSourceKind;
pub(crate) use self::format::{
  read, ButtonState, CursorButton, CursorRecord, CursorSource, CursorStyle, FORMAT_VERSION,
};
const MOVEMENT_INTERVAL: Duration = Duration::from_micros(7_500);
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CursorAppearance {
  pub height: f64,
  pub hotspot_x: f64,
  pub hotspot_y: f64,
  pub style: CursorStyle,
  pub width: f64,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub(super) enum RawCursorEventKind {
  Appearance,
  Move,
  Snapshot,
  Button {
    button: CursorButton,
    click_count: u8,
    state: ButtonState,
  },
}

#[derive(Clone, Debug)]
pub(super) struct RawCursorEvent {
  pub appearance: CursorAppearance,
  pub at: Instant,
  pub kind: RawCursorEventKind,
  pub x: f64,
  pub y: f64,
}

#[derive(Debug)]
struct CursorClock {
  origin: Arc<OnceLock<Instant>>,
  paused_since: Option<Instant>,
  paused_total: Duration,
  running: bool,
}

impl CursorClock {
  fn new(origin: Arc<OnceLock<Instant>>) -> Self {
    Self {
      origin,
      paused_since: None,
      paused_total: Duration::ZERO,
      running: true,
    }
  }

  fn pause(&mut self, at: Instant) {
    if self.paused_since.is_none() {
      self.paused_since = Some(at);
    }
  }

  fn resume(&mut self, at: Instant) {
    if let Some(paused_since) = self.paused_since.take() {
      self.paused_total = self
        .paused_total
        .saturating_add(at.saturating_duration_since(paused_since));
    }
  }

  fn stop(&mut self) {
    self.running = false;
  }

  fn timestamp_us(&self, at: Instant) -> Option<u64> {
    if !self.running || self.paused_since.is_some() {
      return None;
    }
    let origin = *self.origin.get()?;
    let elapsed = at
      .saturating_duration_since(origin)
      .saturating_sub(self.paused_total);
    u64::try_from(elapsed.as_micros()).ok()
  }

  fn initial_timestamp_us(&self) -> Option<u64> {
    (self.running && self.paused_since.is_none() && self.origin.get().is_some()).then_some(0)
  }
}

struct StreamWriter {
  clock: CursorClock,
  failure: Option<String>,
  last_appearance: Option<CursorAppearance>,
  last_flush: Instant,
  last_move: Option<Instant>,
  writer: BufWriter<File>,
}

impl StreamWriter {
  fn write(&mut self, record: &CursorRecord) -> Result<(), String> {
    serde_json::to_writer(&mut self.writer, record).map_err(|error| error.to_string())?;
    self
      .writer
      .write_all(b"\n")
      .map_err(|error| error.to_string())?;
    Ok(())
  }

  fn record(&mut self, event: RawCursorEvent) -> Result<bool, String> {
    let timestamp_us = if matches!(event.kind, RawCursorEventKind::Snapshot) {
      self.clock.initial_timestamp_us()
    } else {
      self.clock.timestamp_us(event.at)
    };
    let Some(timestamp_us) = timestamp_us else {
      return Ok(false);
    };

    if self.last_appearance.as_ref() != Some(&event.appearance) {
      self.write(&CursorRecord::Appearance {
        height: event.appearance.height,
        hotspot_x: event.appearance.hotspot_x,
        hotspot_y: event.appearance.hotspot_y,
        style: event.appearance.style,
        timestamp_us,
        width: event.appearance.width,
      })?;
      self.last_appearance = Some(event.appearance.clone());
    }

    match event.kind {
      RawCursorEventKind::Appearance => {}
      RawCursorEventKind::Move | RawCursorEventKind::Snapshot => {
        if matches!(event.kind, RawCursorEventKind::Move)
          && self
            .last_move
            .is_some_and(|last| event.at.saturating_duration_since(last) < MOVEMENT_INTERVAL)
        {
          return Ok(true);
        }
        self.write(&CursorRecord::Position {
          timestamp_us,
          x: event.x,
          y: event.y,
        })?;
        self.last_move = Some(event.at);
      }
      RawCursorEventKind::Button {
        button,
        click_count,
        state,
      } => {
        self.write(&CursorRecord::Button {
          button,
          click_count,
          state,
          timestamp_us,
          x: event.x,
          y: event.y,
        })?;
        self.writer.flush().map_err(|error| error.to_string())?;
        self.last_flush = event.at;
      }
    }

    if event.at.saturating_duration_since(self.last_flush) >= FLUSH_INTERVAL {
      self.writer.flush().map_err(|error| error.to_string())?;
      self.last_flush = event.at;
    }
    Ok(true)
  }
}

type EventSink = Arc<dyn Fn(RawCursorEvent) -> bool + Send + Sync>;

/// A cursor sidecar being filled beside one native recording.
pub struct CursorRecorder {
  path: PathBuf,
  state: Arc<Mutex<StreamWriter>>,
  stop: Arc<AtomicBool>,
  worker: Option<JoinHandle<()>>,
}

impl CursorRecorder {
  pub fn start(
    path: PathBuf,
    origin: Arc<OnceLock<Instant>>,
    source: CursorSource,
  ) -> Result<Self, String> {
    let file = File::create(&path).map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(
      &mut writer,
      &CursorRecord::Header {
        coordinate_space: "global-logical-points".to_owned(),
        platform: std::env::consts::OS.to_owned(),
        source,
        timebase: "recording-microseconds".to_owned(),
        version: FORMAT_VERSION,
      },
    )
    .map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())?;

    let state = Arc::new(Mutex::new(StreamWriter {
      clock: CursorClock::new(origin),
      failure: None,
      last_appearance: None,
      last_flush: Instant::now(),
      last_move: None,
      writer,
    }));
    let sink_state = Arc::clone(&state);
    let sink: EventSink = Arc::new(move |event| {
      let mut state = sink_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      if state.failure.is_some() {
        return false;
      }
      match state.record(event) {
        Ok(recording) => recording,
        Err(error) => {
          if state.failure.is_none() {
            eprintln!("Cursor recording stopped writing: {error}");
            state.failure = Some(error);
          }
          false
        }
      }
    });
    let stop = Arc::new(AtomicBool::new(false));
    let worker = platform::start(Arc::clone(&stop), sink).inspect_err(|_| {
      let _ = std::fs::remove_file(&path);
    })?;

    Ok(Self {
      path,
      state,
      stop,
      worker: Some(worker),
    })
  }

  pub fn pause(&self, at: Instant) {
    self
      .state
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clock
      .pause(at);
  }

  pub fn resume(&self, at: Instant) {
    self
      .state
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clock
      .resume(at);
  }

  pub fn stop(mut self) -> Result<PathBuf, String> {
    if let Err(error) = self.finish() {
      let _ = std::fs::remove_file(&self.path);
      return Err(error);
    }
    Ok(self.path.clone())
  }

  pub fn cancel(mut self) {
    let _ = self.finish();
    let _ = std::fs::remove_file(&self.path);
  }

  fn finish(&mut self) -> Result<(), String> {
    {
      let mut state = self
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      state.clock.stop();
    }
    self.stop.store(true, Ordering::Release);
    if let Some(worker) = self.worker.take() {
      worker
        .join()
        .map_err(|_| "The cursor recorder stopped unexpectedly".to_owned())?;
    }
    let mut state = self
      .state
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.writer.flush().map_err(|error| error.to_string())?;
    state.failure.take().map_or(Ok(()), Err)
  }
}

impl Drop for CursorRecorder {
  fn drop(&mut self) {
    let _ = self.finish();
  }
}
