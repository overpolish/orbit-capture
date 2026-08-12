// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, RwLock,
  },
  time::{SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, Emitter, Manager, State};

use super::{
  monitor::RecordingMonitor, session::CaptureHandles, RecordingMode, RecordingSnapshot,
  RecordingStatus,
};

const RECORDING_STATE_EVENT: &str = "recording://state";

#[derive(Default)]
pub struct RecordingState {
  pub(super) snapshot: RwLock<RecordingSnapshot>,
  pub(super) handles: Mutex<Option<CaptureHandles>>,
  pub(super) monitor: Arc<RecordingMonitor>,
  generation: AtomicU64,
}

impl RecordingState {
  /// Claims the current start attempt, invalidating any in-flight one.
  pub(super) fn begin_start(&self) -> u64 {
    self
      .generation
      .fetch_add(1, Ordering::SeqCst)
      .wrapping_add(1)
  }

  pub(super) fn is_current(&self, generation: u64) -> bool {
    self.generation.load(Ordering::SeqCst) == generation
  }

  /// Invalidates the in-flight start so a late confirmation is ignored.
  pub(super) fn cancel(&self) {
    self.generation.fetch_add(1, Ordering::SeqCst);
  }
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .ok()
    .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
    .unwrap_or_default()
}

pub(super) fn state(app: &AppHandle) -> State<'_, RecordingState> {
  app.state::<RecordingState>()
}

pub fn snapshot(app: &AppHandle) -> RecordingSnapshot {
  app
    .try_state::<RecordingState>()
    .map(|state| {
      *state
        .snapshot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    })
    .unwrap_or_default()
}

/// Whether the app is outside a recording. Window commands that would re-show
/// hidden chrome consult this before doing anything.
pub fn is_idle(app: &AppHandle) -> bool {
  snapshot(app).status == RecordingStatus::Idle
}

/// The span of the current recording run, i.e. time since the last resume.
fn open_span_ms(snapshot: &RecordingSnapshot, now: u64) -> u64 {
  match snapshot.started_at_ms {
    Some(started_at_ms) if snapshot.status == RecordingStatus::Recording => {
      now.saturating_sub(started_at_ms)
    }
    _ => 0,
  }
}

/// Validates the transition and, only if it is legal, applies it. Pure so the
/// full transition table can be unit tested without an app handle.
pub(super) fn apply_transition(
  snapshot: &mut RecordingSnapshot,
  to: RecordingStatus,
  mode: Option<RecordingMode>,
  now: u64,
) -> Result<(), String> {
  match (snapshot.status, to) {
    (RecordingStatus::Idle, RecordingStatus::Starting) => {
      *snapshot = RecordingSnapshot {
        status: RecordingStatus::Starting,
        mode,
        ..RecordingSnapshot::default()
      };
    }
    (RecordingStatus::Starting, RecordingStatus::Recording) => {
      snapshot.status = RecordingStatus::Recording;
      snapshot.started_at_ms = Some(now);
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Recording, RecordingStatus::Paused) => {
      snapshot.accumulated_ms = snapshot
        .accumulated_ms
        .saturating_add(open_span_ms(snapshot, now));
      snapshot.status = RecordingStatus::Paused;
      snapshot.started_at_ms = None;
      snapshot.paused_at_ms = Some(now);
    }
    (RecordingStatus::Paused, RecordingStatus::Recording) => {
      snapshot.status = RecordingStatus::Recording;
      snapshot.started_at_ms = Some(now);
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Recording | RecordingStatus::Paused, RecordingStatus::Stopping) => {
      snapshot.accumulated_ms = snapshot
        .accumulated_ms
        .saturating_add(open_span_ms(snapshot, now));
      snapshot.status = RecordingStatus::Stopping;
      snapshot.started_at_ms = None;
      snapshot.paused_at_ms = None;
    }
    (RecordingStatus::Starting | RecordingStatus::Stopping, RecordingStatus::Idle) => {
      *snapshot = RecordingSnapshot::default();
    }
    (from, to) => {
      return Err(format!(
        "A recording cannot move from {} to {}",
        from.label(),
        to.label()
      ))
    }
  }

  Ok(())
}

/// Mutates under the lock, drops the guard, and only then emits and touches the
/// tray. Nothing that takes another mutex may run while the guard is alive.
pub(super) fn transition(
  app: &AppHandle,
  to: RecordingStatus,
  mode: Option<RecordingMode>,
) -> Result<RecordingSnapshot, String> {
  let (changed, snapshot) = {
    let state = state(app);
    let mut current = state
      .snapshot
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = *current;
    apply_transition(&mut current, to, mode, now_ms())?;
    (previous != *current, *current)
  };

  if changed {
    let _ = app.emit(RECORDING_STATE_EVENT, snapshot);

    #[cfg(desktop)]
    crate::tray::apply_recording_status(app, snapshot.status);
  }

  Ok(snapshot)
}

pub(super) fn set_countdown(app: &AppHandle, seconds: u8) {
  let snapshot = {
    let state = state(app);
    let mut snapshot = state
      .snapshot
      .write()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    snapshot.countdown_seconds_remaining = seconds;
    *snapshot
  };
  let _ = app.emit(RECORDING_STATE_EVENT, snapshot);
}
