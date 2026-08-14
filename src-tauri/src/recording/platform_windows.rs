// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows screen recording: Windows Graphics Capture into Media Foundation.
//!
//! WGC hands D3D11 textures to a bounded channel. The capture callback never
//! waits for the encoder, and Media Foundation consumes those textures through
//! its DXGI device manager without a GPU-to-CPU copy or an FFmpeg subprocess.

mod capture;
mod writer;

use std::sync::{mpsc, Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use capture::CaptureObjects;
use writer::{Command, WriterConfig};

use super::encoding::FinalizeInfo;
use super::{
  cursor::{CursorSource, CursorSourceKind},
  CaptureStartupConfig, PrimaryCaptureSource,
};

const FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct CaptureStart {
  pub cursor_source: Option<super::cursor::CursorSource>,
  pub first_frame: mpsc::Receiver<Result<(), String>>,
  pub session: CaptureSession,
  pub source_scale_factor: f32,
  pub timeline_origin: Arc<OnceLock<Instant>>,
}

pub struct CaptureSession {
  capture: Option<CaptureObjects>,
  commands: mpsc::SyncSender<Command>,
  stopped_at: Arc<OnceLock<Instant>>,
  worker: Option<JoinHandle<()>>,
}

impl CaptureSession {
  pub fn mark_stopped_at(&self, at: Instant) {
    let _ = self.stopped_at.set(at);
  }

  pub fn pause_at(&self, at: Instant) {
    let _ = self.commands.send(Command::Pause(at));
  }

  pub fn resume_at(&self, at: Instant) -> Result<(), String> {
    self
      .commands
      .send(Command::Resume(at))
      .map_err(|_| "The recording is no longer running".to_owned())
  }

  pub fn stop_at(mut self, at: Instant) -> Result<FinalizeInfo, String> {
    self.close_capture();
    let (reply, replies) = mpsc::channel();
    self
      .commands
      .send(Command::Stop { at, reply })
      .map_err(|_| "The recording is no longer running".to_owned())?;
    let result = replies
      .recv_timeout(FINALIZE_TIMEOUT)
      .map_err(|_| "The recording did not finish in time".to_owned())?;
    self.join_writer();
    result
  }

  pub fn cancel(mut self) {
    self.shutdown();
  }

  fn close_capture(&mut self) {
    if let Some(mut capture) = self.capture.take() {
      capture.close();
    }
  }

  fn join_writer(&mut self) {
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }

  fn shutdown(&mut self) {
    self.close_capture();
    let _ = self.commands.send(Command::Cancel);
    self.join_writer();
  }
}

impl Drop for CaptureSession {
  fn drop(&mut self) {
    self.shutdown();
  }
}

pub fn begin_blocking(config: CaptureStartupConfig) -> Result<CaptureStart, String> {
  let CaptureStartupConfig {
    camera,
    camera_path,
    include_own_windows: _,
    microphone_id,
    monitor: _,
    on_failure,
    path,
    primary,
    system_audio,
  } = config;
  if camera.is_some() || camera_path.is_some() {
    return Err("Camera recording is not yet available on Windows".to_owned());
  }
  if microphone_id.is_some() {
    return Err("Microphone recording is not yet available on Windows".to_owned());
  }
  if system_audio.enabled {
    return Err("System audio recording is not yet available on Windows".to_owned());
  }
  let PrimaryCaptureSource::Screen {
    fps,
    monitor_id,
    show_cursor,
  } = primary
  else {
    return Err("Only full-screen recording is available on Windows right now".to_owned());
  };

  let monitor = xcap::Monitor::all()
    .map_err(|error| error.to_string())?
    .into_iter()
    .find(|monitor| monitor.id().ok() == Some(monitor_id))
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
  let source_scale_factor = monitor.scale_factor().map_err(|error| error.to_string())?;
  let monitor_x = monitor.x().map_err(|error| error.to_string())?;
  let monitor_y = monitor.y().map_err(|error| error.to_string())?;
  let width = monitor.width().map_err(|error| error.to_string())? & !1;
  let height = monitor.height().map_err(|error| error.to_string())? & !1;
  if width < 2 || height < 2 {
    return Err("The selected monitor has no recordable area".to_owned());
  }

  let timeline_origin = Arc::new(OnceLock::new());
  let (commands, command_rx) = mpsc::sync_channel(8);
  let (first_frame_tx, first_frame) = mpsc::channel();
  let device = capture::create_device()?;
  let writer_device = device.clone();
  let writer_origin = Arc::clone(&timeline_origin);
  let stopped_at = Arc::new(OnceLock::new());
  let writer_stopped_at = Arc::clone(&stopped_at);
  let worker = std::thread::Builder::new()
    .name("orbit-windows-recording-writer".to_owned())
    .spawn(move || {
      writer::run(
        WriterConfig {
          device: writer_device,
          fps,
          height,
          on_failure,
          path,
          stopped_at: writer_stopped_at,
          timeline_origin: writer_origin,
          width,
        },
        command_rx,
        first_frame_tx,
      );
    })
    .map_err(|error| error.to_string())?;

  // Match the user's capture choice exactly. The semantic sidecar is recorded
  // independently, so native cursor pixels and the editable cursor layer may
  // intentionally coexist when baking is enabled.
  let capture = match CaptureObjects::start(device, monitor_id, show_cursor, commands.clone()) {
    Ok(capture) => capture,
    Err(error) => {
      let _ = commands.send(Command::Cancel);
      let _ = worker.join();
      return Err(error);
    }
  };

  Ok(CaptureStart {
    cursor_source: Some(CursorSource {
      height: f64::from(height),
      kind: CursorSourceKind::Screen,
      platform_id: monitor_id.to_string(),
      video_height: height,
      video_width: width,
      width: f64::from(width),
      x: f64::from(monitor_x),
      y: f64::from(monitor_y),
    }),
    first_frame,
    session: CaptureSession {
      capture: Some(capture),
      commands,
      stopped_at,
      worker: Some(worker),
    },
    source_scale_factor,
    timeline_origin,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::recording::{monitor::RecordingMonitor, SystemAudioSelection};

  #[test]
  #[ignore = "requires an interactive Windows display and hardware encoder"]
  fn records_a_playable_screen_sample() {
    let monitor = xcap::Monitor::all().unwrap().into_iter().next().unwrap();
    let monitor_id = monitor.id().unwrap();
    let path = std::env::temp_dir().join(format!(
      "orbit-capture-windows-recording-{}.mp4",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let start = begin_blocking(CaptureStartupConfig {
      camera: None,
      camera_path: None,
      include_own_windows: true,
      microphone_id: None,
      monitor: Arc::new(RecordingMonitor::default()),
      on_failure: Arc::new(|error| eprintln!("recording failure: {error}")),
      path: path.clone(),
      primary: PrimaryCaptureSource::Screen {
        fps: 60,
        monitor_id,
        show_cursor: true,
      },
      system_audio: SystemAudioSelection::default(),
    })
    .unwrap();
    start
      .first_frame
      .recv_timeout(Duration::from_secs(5))
      .unwrap()
      .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let stopped_at = Instant::now();
    start.session.mark_stopped_at(stopped_at);
    // Reproduce a busy async finalizer: frames may keep arriving during this
    // delay, but none may extend the recording past the user's stop instant.
    std::thread::sleep(Duration::from_secs(3));
    let info = start.session.stop_at(stopped_at).unwrap();
    assert!(
      info.duration_ms >= 900,
      "duration was {} ms",
      info.duration_ms
    );
    assert!(
      info.duration_ms <= 1_500,
      "stop finalization added a frozen tail: {} ms",
      info.duration_ms
    );
    assert!(std::fs::metadata(&path).unwrap().len() > 1_024);
    std::fs::remove_file(path).unwrap();
  }
}
