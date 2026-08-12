// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  path::PathBuf,
  sync::{mpsc::Receiver, Arc},
  time::{Duration, Instant},
};

use chrono::{Local, NaiveDateTime};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{
  capture, cursor::CursorRecorder, encoding, snapshot, state, CameraCaptureMode,
  CaptureStartupConfig, FinalizeInfo, PrimaryCaptureSource, RecordingMode, RecordingStatus,
  StartRecordingOptions, SystemAudioSelection,
};

const RECORDING_ERROR_EVENT: &str = "recording://error";
/// The folder working files are written to, under the app's data directory.
const RECORDINGS_DIRECTORY: &str = "Recordings";
/// How long a start may go without producing a frame before it is called a
/// failure. Permission prompts and display wake-ups are the slow cases and
/// both resolve well inside this.
pub(super) const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// Everything a running recording is, from the state machine's side: a live
/// capture session and the file it is filling.
pub(super) struct CaptureHandles {
  cursor: Option<CursorRecorder>,
  output_path: PathBuf,
  session: capture::CaptureSession,
  source_scale_factor: f32,
  /// Stamped when capture begins, so the suggested file name reads as the
  /// moment the user started rather than the moment they stopped.
  started_at: NaiveDateTime,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingErrorPayload {
  phase: &'static str,
  message: String,
}

pub(super) fn emit_error(app: &AppHandle, phase: &'static str, message: &str) {
  eprintln!("Recording {phase} failed: {message}");
  let _ = app.emit(
    RECORDING_ERROR_EVENT,
    RecordingErrorPayload {
      phase,
      message: message.to_owned(),
    },
  );
}

pub(super) fn require_status(
  app: &AppHandle,
  allowed: &[RecordingStatus],
  action: &str,
) -> Result<RecordingStatus, String> {
  let status = snapshot(app).status;
  if allowed.contains(&status) {
    Ok(status)
  } else {
    Err(format!(
      "A recording that is {} cannot {action}",
      status.label()
    ))
  }
}

pub(super) fn take_handles(app: &AppHandle) -> Option<CaptureHandles> {
  state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take()
}

pub(super) fn store_handles(app: &AppHandle, handles: CaptureHandles) {
  *state(app)
    .handles
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(handles);
}

// ---------------------------------------------------------------------------
// Capture. Every entry point here is called from a blocking task, never from
// the thread that services the UI, and never with a recording lock held.
// ---------------------------------------------------------------------------

/// Where working files live: inside the app's own data directory, so a
/// recording that is never saved leaves nothing in a folder the user looks at.
pub fn recordings_directory(app: &AppHandle) -> Result<PathBuf, String> {
  app
    .path()
    .app_data_dir()
    .map(|directory| directory.join(RECORDINGS_DIRECTORY))
    .map_err(|error| error.to_string())
}

pub(super) fn records_cursor(mode: RecordingMode) -> bool {
  matches!(
    mode,
    RecordingMode::Screen | RecordingMode::Region | RecordingMode::Window
  )
}

/// Defence in depth, run before anything is hidden or transitioned. The Record
/// button is already gated on a selected source, but a mode without one could
/// never produce a file.
pub(super) fn validate_options(options: &StartRecordingOptions) -> Result<(), String> {
  match options.mode {
    RecordingMode::Screen | RecordingMode::Region if options.monitor_id.is_none() => {
      Err("No monitor is selected to record".to_owned())
    }
    RecordingMode::Region if options.region.is_none() => {
      Err("No region is selected to record".to_owned())
    }
    RecordingMode::Window if options.window_id.is_none() => {
      Err("No window is selected to record".to_owned())
    }
    RecordingMode::Audio if !options.system_audio && options.microphone_id.is_none() => {
      Err("No audio source is selected to record".to_owned())
    }
    RecordingMode::Camera if options.camera_id.is_none() => {
      Err("No camera is selected to record".to_owned())
    }
    _ if options.camera_id.is_some()
      && (options.camera_width.is_none()
        || options.camera_height.is_none()
        || options.camera_fps.is_none()) =>
    {
      Err("The selected camera mode is incomplete".to_owned())
    }
    _ => Ok(()),
  }
}

/// Opens the capture and the file behind it. Blocking, and slow enough to be
/// worth keeping off the thread that draws.
pub(super) fn begin_capture(
  app: &AppHandle,
  options: &StartRecordingOptions,
) -> Result<(CaptureHandles, Receiver<Result<(), String>>), String> {
  let camera_primary = options.mode == RecordingMode::Camera;
  let camera = options
    .camera_id
    .as_ref()
    .map(|device_id| CameraCaptureMode {
      device_id: device_id.clone(),
      flipped: options.camera_flipped,
      fps: options.camera_fps.expect("validated above"),
      height: options.camera_height.expect("validated above"),
      width: options.camera_width.expect("validated above"),
    });
  let directory = recordings_directory(app)?;
  std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
  let started_at = Local::now().naive_local();
  let output_path = directory.join(if options.mode == RecordingMode::Audio {
    encoding::audio_temp_file_name(started_at)
  } else {
    encoding::temp_file_name(started_at)
  });
  let camera_path = options
    .camera_id
    .as_ref()
    .filter(|_| !camera_primary)
    .map(|_| directory.join(encoding::camera_temp_file_name(started_at)));
  // `show_cursor` controls only the pixels ScreenCaptureKit burns into the
  // movie. The independent cursor track is always useful: hiding the native
  // pointer is how a clean recording gets Orbit Capture's dynamic cursor.
  let cursor_path = records_cursor(options.mode)
    .then(|| directory.join(encoding::cursor_temp_file_name(started_at)));

  // Reported at most once per recording, from the writer thread, however many
  // frames the failure goes on to affect.
  let reporter = app.clone();
  let on_failure = std::sync::Arc::new(move |reason: String| {
    emit_error(&reporter, "capture", &reason);
  });

  crate::camera_preview::stop_all(app);
  let monitor = Arc::clone(&state(app).monitor);
  monitor.configure(
    options.system_audio,
    options.microphone_id.is_some(),
    options.camera_id.is_some(),
  );
  let primary = match options.mode {
    RecordingMode::Screen => PrimaryCaptureSource::Screen {
      fps: options.fps,
      monitor_id: options.monitor_id.expect("validated above"),
      show_cursor: options.show_cursor,
    },
    RecordingMode::Region => PrimaryCaptureSource::Region {
      fps: options.fps,
      monitor_id: options.monitor_id.expect("validated above"),
      region: options.region.expect("validated above"),
      show_cursor: options.show_cursor,
    },
    RecordingMode::Window => PrimaryCaptureSource::Window {
      fps: options.fps,
      show_cursor: options.show_cursor,
      window_id: options.window_id.expect("validated above"),
    },
    RecordingMode::Camera => PrimaryCaptureSource::Camera,
    RecordingMode::Audio => PrimaryCaptureSource::Audio,
  };
  let capture::CaptureStart {
    cursor_source,
    first_frame,
    session,
    source_scale_factor,
    timeline_origin,
  } = capture::begin_blocking(CaptureStartupConfig {
    camera,
    camera_path: camera_path.clone(),
    microphone_id: options.microphone_id.clone(),
    monitor,
    on_failure,
    path: output_path.clone(),
    primary,
    system_audio: SystemAudioSelection {
      application_ids: options.system_audio_application_ids.clone(),
      enabled: options.system_audio,
      process_ids: options.system_audio_process_ids.clone(),
    },
  })
  .inspect_err(|_| {
    // A start that never got going leaves an empty container behind.
    let _ = std::fs::remove_file(&output_path);
    if let Some(camera_path) = &camera_path {
      let _ = std::fs::remove_file(camera_path);
    }
  })?;

  let cursor = match (cursor_path, cursor_source) {
    (Some(path), Some(source)) => match CursorRecorder::start(path, timeline_origin, source) {
      Ok(cursor) => Some(cursor),
      Err(error) => {
        session.cancel();
        let _ = std::fs::remove_file(&output_path);
        if let Some(camera_path) = &camera_path {
          let _ = std::fs::remove_file(camera_path);
        }
        return Err(error);
      }
    },
    (None, _) => None,
    (Some(_), None) => {
      session.cancel();
      let _ = std::fs::remove_file(&output_path);
      if let Some(camera_path) = &camera_path {
        let _ = std::fs::remove_file(camera_path);
      }
      return Err("The capture source has no cursor coordinate space".to_owned());
    }
  };

  Ok((
    CaptureHandles {
      cursor,
      output_path,
      session,
      source_scale_factor,
      started_at,
    },
    first_frame,
  ))
}

pub(super) fn pause_capture(handles: &CaptureHandles) {
  let at = Instant::now();
  handles.session.pause_at(at);
  if let Some(cursor) = &handles.cursor {
    cursor.pause(at);
  }
}

pub(super) fn resume_capture(handles: &CaptureHandles) -> Result<(), String> {
  let at = Instant::now();
  handles.session.resume_at(at)?;
  if let Some(cursor) = &handles.cursor {
    cursor.resume(at);
  }
  Ok(())
}

/// Finishes the movie, returning it alongside the name to suggest for it.
pub(super) fn finalize_capture(handles: CaptureHandles) -> Result<(FinalizeInfo, String), String> {
  let CaptureHandles {
    cursor,
    output_path,
    session,
    source_scale_factor,
    started_at,
  } = handles;

  let at = Instant::now();
  let cursor_path = cursor.map(CursorRecorder::stop).transpose();
  let mut info = match session.stop_at(at) {
    Ok(info) => info,
    Err(error) => {
      // Nothing playable came out, so nothing is left lying around either.
      let _ = std::fs::remove_file(&output_path);
      if let Ok(Some(path)) = &cursor_path {
        let _ = std::fs::remove_file(path);
      }
      return Err(error);
    }
  };
  info.cursor_path = match cursor_path {
    Ok(path) => path,
    Err(error) => {
      let _ = std::fs::remove_file(&info.path);
      if let Some(camera) = &info.camera {
        let _ = std::fs::remove_file(&camera.path);
      }
      return Err(error);
    }
  };
  info.source_scale_factor = source_scale_factor;

  Ok((info, crate::screenshots::capture_file_stem(started_at)))
}

pub(super) fn discard_capture(handles: Option<CaptureHandles>) {
  let Some(CaptureHandles {
    cursor,
    output_path,
    session,
    ..
  }) = handles
  else {
    return;
  };

  session.cancel();
  if let Some(cursor) = cursor {
    cursor.cancel();
  }
  let _ = std::fs::remove_file(output_path);
}
