// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod encoding;
#[cfg(target_os = "macos")]
mod microphone;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(not(target_os = "macos"))]
mod platform_unsupported;
mod session;
mod state;
mod types;
mod ui;

use tauri::{AppHandle, State};

use crate::windows;

#[cfg(target_os = "macos")]
use platform as capture;
#[cfg(not(target_os = "macos"))]
use platform_unsupported as capture;

pub use encoding::FinalizeInfo;
pub use session::recordings_directory;
pub use state::{is_idle, snapshot, RecordingState};
pub use types::{
  RecordingMode, RecordingSnapshot, RecordingStatus, Region, StartRecordingOptions,
  SystemAudioSelection,
};

use session::{
  begin_capture, discard_capture, emit_error, finalize_capture, pause_capture, require_status,
  resume_capture, store_handles, take_handles, validate_options, FIRST_FRAME_TIMEOUT,
};
#[cfg(test)]
use state::apply_transition;
use state::{state, transition};
#[cfg(test)]
use types::DEFAULT_FPS;
use ui::{prepare_windows, restore_windows, show_recording_ui};

// ---------------------------------------------------------------------------
// Lifecycle. Each entry point validates before it causes any side effect, and
// is callable from both the commands below and the tray menu.
// ---------------------------------------------------------------------------

/// Unwinds a start that could not be completed, from wherever it failed.
fn abandon_start(app: &AppHandle, error: &str) {
  emit_error(app, "start", error);
  state(app).cancel();
  discard_capture(take_handles(app));
  restore_windows(app);
  let _ = transition(app, RecordingStatus::Idle, None);
  show_recording_ui(app);
}

pub fn start(app: &AppHandle, options: StartRecordingOptions) -> Result<(), String> {
  validate_options(&options)?;
  // A second start while `Starting` is rejected here, not merely by a
  // disabled button.
  transition(app, RecordingStatus::Starting, Some(options.mode))?;
  let generation = state(app).begin_start();

  if let Err(error) = prepare_windows(app, &options) {
    abandon_start(app, &error);
    return Err(error);
  }

  let app = app.clone();
  // Opening a capture talks to the window server and waits on it. `tokio` is
  // macOS-only in this crate, so this is a blocking task the way finalize is -
  // and either way it must not run on the thread that draws.
  tauri::async_runtime::spawn_blocking(move || {
    if !state(&app).is_current(generation) {
      return;
    }

    let (handles, first_frame) = match begin_capture(&app, &options) {
      Ok(started) => started,
      Err(error) => return abandon_start(&app, &error),
    };
    // Cancelling while the capture was opening: the handles were never
    // stored, so this is the only place that can still tear them down.
    if !state(&app).is_current(generation) {
      return discard_capture(Some(handles));
    }
    store_handles(&app, handles);

    // Nothing is recording until a frame has actually been written. Moving to
    // `Recording` any earlier would start a clock the file cannot honour.
    let confirmed = first_frame
      .recv_timeout(FIRST_FRAME_TIMEOUT)
      .unwrap_or_else(|_| Err("The recording produced no frames".to_owned()));
    if !state(&app).is_current(generation) {
      // Cancelling usually takes the handles itself, but it can land in the
      // instant between the check above and the store, in which case they are
      // still here and nothing else will ever come back for them.
      return discard_capture(take_handles(&app));
    }

    match confirmed {
      Ok(()) => {
        if let Err(error) = transition(&app, RecordingStatus::Recording, None) {
          emit_error(&app, "start", &error);
          return;
        }
        if let Err(error) = windows::show_recording_dock(&app) {
          emit_error(&app, "start", &error.to_string());
        }
      }
      Err(error) => abandon_start(&app, &error),
    }
  });

  Ok(())
}

pub fn pause(app: &AppHandle) -> Result<(), String> {
  transition(app, RecordingStatus::Paused, None).inspect_err(|error| {
    emit_error(app, "pause", error);
  })?;

  if let Some(handles) = state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_ref()
  {
    pause_capture(handles);
  }

  Ok(())
}

pub fn resume(app: &AppHandle) -> Result<(), String> {
  require_status(app, &[RecordingStatus::Paused], "resume").inspect_err(|error| {
    emit_error(app, "resume", error);
  })?;

  let resumed = {
    let state = state(app);
    let handles = state
      .handles
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    handles.as_ref().map_or(Ok(()), resume_capture)
  };
  if let Err(error) = resumed {
    emit_error(app, "resume", &error);
    return Err(error);
  }

  transition(app, RecordingStatus::Recording, None).inspect_err(|error| {
    emit_error(app, "resume", error);
  })?;

  Ok(())
}

pub fn toggle_pause(app: &AppHandle) -> Result<(), String> {
  if snapshot(app).status == RecordingStatus::Paused {
    resume(app)
  } else {
    pause(app)
  }
}

pub fn stop(app: &AppHandle) -> Result<(), String> {
  transition(app, RecordingStatus::Stopping, None).inspect_err(|error| {
    emit_error(app, "stop", error);
  })?;
  state(app).cancel();

  let app = app.clone();
  // `tokio` is macOS-only in this crate, so the finalize wait uses a blocking
  // task the way the window animations do.
  tauri::async_runtime::spawn_blocking(move || {
    let finalized = take_handles(&app).map(finalize_capture);

    // The chrome comes back before the export window opens, so the export
    // window is the last thing raised and therefore the frontmost.
    restore_windows(&app);
    if let Err(error) = transition(&app, RecordingStatus::Idle, None) {
      emit_error(&app, "stop", &error);
    }
    show_recording_ui(&app);

    match finalized {
      Some(Ok((info, suggested_file_stem))) => {
        if let Err(error) = crate::exports::present_recording(&app, info, suggested_file_stem) {
          emit_error(&app, "stop", &error);
        }
      }
      Some(Err(error)) => emit_error(&app, "stop", &error),
      None => {}
    }
  });

  Ok(())
}

pub fn cancel(app: &AppHandle) -> Result<(), String> {
  let status = require_status(
    app,
    &[
      RecordingStatus::Starting,
      RecordingStatus::Recording,
      RecordingStatus::Paused,
    ],
    "be discarded",
  )?;

  if matches!(status, RecordingStatus::Recording | RecordingStatus::Paused) {
    transition(app, RecordingStatus::Stopping, None)?;
  }
  state(app).cancel();

  discard_capture(take_handles(app));
  restore_windows(app);
  transition(app, RecordingStatus::Idle, None)?;
  show_recording_ui(app);

  Ok(())
}

#[tauri::command]
pub fn get_recording_snapshot(state: State<'_, RecordingState>) -> RecordingSnapshot {
  *state
    .snapshot
    .read()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
pub fn start_recording(app: AppHandle, options: StartRecordingOptions) -> Result<(), String> {
  start(&app, options)
}

#[tauri::command]
pub fn pause_recording(app: AppHandle) -> Result<(), String> {
  pause(&app)
}

#[tauri::command]
pub fn resume_recording(app: AppHandle) -> Result<(), String> {
  resume(&app)
}

#[tauri::command]
pub fn stop_recording(app: AppHandle) -> Result<(), String> {
  stop(&app)
}

#[tauri::command]
pub fn cancel_recording(app: AppHandle) -> Result<(), String> {
  cancel(&app)
}

#[cfg(test)]
mod tests;
