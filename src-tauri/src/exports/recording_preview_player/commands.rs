// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::exports::CameraOverlaySettings;
use tauri::{ipc::Channel, AppHandle, Emitter, Manager};

use super::*;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingPreviewSelectionChangeEvent {
  pane_index: Option<u32>,
  session_id: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingPreviewSelectionGestureEvent {
  camera_overlay: Option<CameraOverlaySettings>,
  delta_x: f64,
  delta_y: f64,
  edges: u32,
  operation: u32,
  pane_index: u32,
  phase: &'static str,
  recording_output: Option<RecordingOutputSettings>,
  scale: f64,
  session_id: u64,
}

#[tauri::command]
pub async fn start_recording_preview_player(
  app: AppHandle,
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  artifact_id: u64,
  settings: PreviewPlayerSettings,
  frame_channel: Channel,
  event_channel: Channel<RecordingPreviewPlayerEvent>,
  session_id: u64,
) -> Result<RecordingPreviewPlayerInfo, String> {
  let mut sources = sources(&app, artifact_id, Some(&settings))?;
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  if let Some(surface) = sources.preview_surface.as_mut() {
    let surface = Arc::get_mut(surface)
      .ok_or_else(|| "The recording preview surface is already in use".to_owned())?;
    let event_app = app.clone();
    surface.enable_editor(Box::new(move |zoom_percent| {
      let _ = event_app.emit(
        "recording-preview://transform",
        RecordingPreviewTransformEvent {
          session_id,
          zoom_percent,
        },
      );
    }));
    let event_app = app.clone();
    surface.set_selection_callback(Box::new(move |pane_index| {
      let _ = event_app.emit(
        "recording-preview://selection-change",
        RecordingPreviewSelectionChangeEvent {
          pane_index,
          session_id,
        },
      );
    }));
    let event_app = app.clone();
    surface.set_selection_gesture_callback(Box::new(
      move |phase, pane_index, operation, edges, scale, delta_x, delta_y| {
        let manager = event_app.state::<RecordingPreviewPlayerState>();
        let composition = if let Ok(mut manager) = manager.0.try_lock() {
          let updated = manager
            .handle_selection_gesture(phase, pane_index, operation, edges, scale, delta_x, delta_y)
            .is_ok();
          // A canvas resize - a Frame drag, or an Alt-drag Move growing the
          // canvas around its layer - moves the pane box on every pointer
          // move, so the composition has to follow it in the same input. macOS
          // recomposes its retained workspace natively; Windows redraws the
          // paused still from its cached sources here, which also publishes
          // the geometry the drag deferred. Without this the pane would show
          // the previous canvas letterboxed into the new box until mouse-up.
          #[cfg(target_os = "windows")]
          if updated
            && (operation == super::super::preview_platform::SelectionGestureOperation::FrameResize
              || (operation == super::super::preview_platform::SelectionGestureOperation::Move
                && edges & super::AUTO_FIT_MOVE_EDGE != 0))
            && !matches!(
              phase,
              super::super::preview_platform::SelectionGesturePhase::Begin
            )
          {
            // The surface state lock is not held here (`handle_editor_input`
            // emits its gestures after that scope ends), and nothing under
            // this call re-enters the manager, so the manager lock is only
            // ever taken before the surface lock - the same order every
            // command uses.
            let _ = manager.redraw_still_now();
          }
          #[cfg(not(target_os = "windows"))]
          let _ = updated;
          manager.selection_composition()
        } else {
          None
        };
        let phase = match phase {
          super::super::preview_platform::SelectionGesturePhase::Begin => "begin",
          super::super::preview_platform::SelectionGesturePhase::Update => "update",
          super::super::preview_platform::SelectionGesturePhase::End => "end",
          super::super::preview_platform::SelectionGesturePhase::Cancel => "cancel",
        };
        let operation = match operation {
          super::super::preview_platform::SelectionGestureOperation::Move => 0,
          super::super::preview_platform::SelectionGestureOperation::Resize => 1,
          super::super::preview_platform::SelectionGestureOperation::Radius => 2,
          super::super::preview_platform::SelectionGestureOperation::FrameResize => 3,
          super::super::preview_platform::SelectionGestureOperation::FrameRadius => 4,
          super::super::preview_platform::SelectionGestureOperation::CropMove => 5,
          super::super::preview_platform::SelectionGestureOperation::CropResize => 6,
        };
        let _ = event_app.emit(
          "recording-preview://selection-gesture",
          RecordingPreviewSelectionGestureEvent {
            camera_overlay: composition.as_ref().map(|value| value.camera_overlay),
            delta_x,
            delta_y,
            edges,
            operation,
            pane_index,
            phase,
            recording_output: composition.map(|value| value.recording_output),
            scale,
            session_id,
          },
        );
      },
    ));
    surface.set_selection_snapping(true);
    surface.set_editor_active(false);
  }
  let info = RecordingPreviewPlayerInfo {
    duration_ms: sources.duration_ms,
    layout: sources.layout.clone(),
  };
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  if session_id < manager.latest_session_id {
    return Ok(info);
  }
  manager.stop();
  manager.latest_session_id = session_id;
  manager.artifact_id = Some(artifact_id);
  manager.audio_indices = settings.audio.enabled_stream_indices;
  manager.audio_volumes = settings.audio.audio_track_volumes;
  manager.event_channel = Some(event_channel);
  manager.frame_channel = Some(frame_channel);
  manager.latest_layout_request = 0;
  manager.latest_seek_request = 0;
  manager.position_ms = 0;
  manager.sources = Some(sources);
  manager.session_id = Some(session_id);
  manager.restart(PlaybackMode::Still)?;
  Ok(info)
}

// Async for the same reason as `layout_recording_preview_surface`: a paused
// redraw must not block the main thread's input pump.
#[tauri::command]
pub async fn set_recording_preview_composition(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  recording_output: RecordingOutputSettings,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  let settings = manager
    .sources
    .as_ref()
    .ok_or_else(|| "The recording preview player is not open".to_owned())?
    .composition_settings
    .clone()
    .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?;
  let next = PreviewCompositionSettings {
    bake_camera,
    camera_overlay: camera_overlay.clone(),
    recording_output: recording_output.clone(),
  };
  let current = settings
    .read()
    .map_err(|_| "The recording preview composition is unavailable".to_owned())?;
  if *current == next {
    return Ok(());
  }
  #[cfg(target_os = "windows")]
  let bake_changed = current.bake_camera != bake_camera;
  drop(current);
  *settings
    .write()
    .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
  if !manager.is_playing {
    // A composition change leaves the decoded frame and its cursor valid, so
    // Windows redraws the paused still synchronously from the cached sources
    // instead of restarting the decoder; a full reopen per pointer move made
    // camera drags trail the pointer. A bake toggle keeps the decoder path -
    // the newly active mode's source cache is absent or stale - and so do
    // cursor-effect changes, whose composition is stale by definition.
    #[cfg(target_os = "windows")]
    if !bake_changed {
      let has_camera = manager
        .sources
        .as_ref()
        .is_some_and(|sources| sources.camera_path.is_some());
      let redrawn = manager
        .sources
        .as_ref()
        .and_then(|sources| sources.preview_surface.as_ref())
        .is_some_and(|surface| {
          surface
            .redraw_still(
              bake_camera && has_camera,
              &recording_output.primary,
              &recording_output.camera,
              camera_overlay,
              recording_output.camera.drop_shadow,
              recording_output.camera_on_top,
            )
            .unwrap_or(false)
        });
      if redrawn {
        return Ok(());
      }
    }
    manager.restart(PlaybackMode::InteractiveStill)?;
  }
  Ok(())
}

#[tauri::command]
pub fn set_recording_preview_cursor_effects(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  cursor_effects: CursorEffectSettings,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  let settings = manager
    .sources
    .as_ref()
    .ok_or_else(|| "The recording preview player is not open".to_owned())?
    .cursor_settings
    .clone();
  *settings
    .write()
    .map_err(|_| "The cursor preview settings are unavailable".to_owned())? = cursor_effects;
  if !manager.is_playing {
    manager.restart(PlaybackMode::InteractiveStill)?;
  }
  Ok(())
}

/// Playback teardown kills the ffmpeg audio child, drops the CoreAudio output
/// stream and joins the decode threads, which together take the better part of
/// a second. Never do that on the caller's thread: these commands run on the
/// macOS main thread, and WebKit's layer commits go through it, so the webview
/// visibly freezes for as long as the join lasts.
async fn cancel_off_thread(worker: PreviewPlayerWorker) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || {
    // The position the caller already read from the atomic is at most one
    // frame behind what the join reports, so the returned value is dropped.
    worker.cancel();
  })
  .await
  .map_err(|error| error.to_string())
}

// Async so the old worker's join stays off the main thread. It has to finish
// before the new worker starts, though: two cpal output streams must not own
// the audio device at once.
#[tauri::command]
pub async fn play_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let worker = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The recording preview player is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.is_playing = true;
    manager.take_worker()
  };
  if let Some(worker) = worker {
    cancel_off_thread(worker).await?;
  }
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if !manager.is_playing {
    // A pause or a seek won the race while the old worker was being joined,
    // and it already owns what the preview presents.
    return Ok(());
  }
  manager.restart(PlaybackMode::Playing)
}

#[tauri::command]
pub async fn pause_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let (worker, result) = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The recording preview player is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.is_playing = false;
    // Taking the worker signals it - so it stops presenting frames before the
    // still decoder starts - and records the position it displayed, without
    // waiting for its threads. The still decoder is an independent reader, so
    // the paused frame decodes in parallel with that teardown.
    let worker = manager.take_worker();
    let displayed_position_ms = manager.position_ms;
    let duration_ms = manager
      .sources
      .as_ref()
      .map_or(0, |sources| sources.duration_ms);
    manager.position_ms = decodable_position(manager.position_ms, duration_ms);
    if let Some(channel) = &manager.event_channel {
      let _ = channel.send(RecordingPreviewPlayerEvent::Paused {
        position_ms: displayed_position_ms,
      });
    }
    (worker, manager.restart(PlaybackMode::InteractiveStill))
  };
  if let Some(worker) = worker {
    cancel_off_thread(worker).await?;
  }
  result
}

fn decodable_position(position_ms: u64, duration_ms: u64) -> u64 {
  position_ms.min(duration_ms.saturating_sub(1))
}

#[cfg(test)]
mod tests {
  use super::decodable_position;

  #[test]
  fn playback_end_uses_the_final_decodable_offset_for_its_still() {
    assert_eq!(decodable_position(8_000, 8_000), 7_999);
    assert_eq!(decodable_position(4_000, 8_000), 4_000);
    assert_eq!(decodable_position(0, 0), 0);
  }
}

#[tauri::command]
pub async fn seek_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  position_ms: u64,
  request_id: u64,
  rough: bool,
  selection_visible: Option<bool>,
  session_id: u64,
) -> Result<(), String> {
  // The request-id guard and the restart stay in one lock scope, so two
  // overlapping seeks still order correctly; only the joins - of separate,
  // already signalled workers - happen afterwards and off this thread.
  let (worker, result) = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The recording preview player is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    if request_id < manager.latest_seek_request {
      return Ok(());
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if let Some(visible) = selection_visible {
      if let Some(surface) = manager
        .sources
        .as_ref()
        .and_then(|sources| sources.preview_surface.as_ref())
      {
        surface.set_selection_visible(visible);
      }
    }
    manager.latest_seek_request = request_id;
    manager.rough_seek = rough;
    let worker = manager.take_worker();
    let duration_ms = manager
      .sources
      .as_ref()
      .map_or(0, |value| value.duration_ms);
    manager.position_ms = position_ms.min(duration_ms.saturating_sub(1));
    manager.is_playing = false;
    (worker, manager.restart(PlaybackMode::InteractiveStill))
  };
  if let Some(worker) = worker {
    cancel_off_thread(worker).await?;
  }
  result
}

#[tauri::command]
pub fn select_recording_preview_audio(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  enabled_stream_indices: Vec<usize>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  manager.audio_indices.clone_from(&enabled_stream_indices);
  if let Some(worker) = &manager.worker {
    worker.select_audio(enabled_stream_indices)?;
  }
  Ok(())
}

#[tauri::command]
pub fn set_recording_preview_audio_volumes(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  audio_track_volumes: Vec<AudioTrackVolume>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  manager.audio_volumes.clone_from(&audio_track_volumes);
  if let Some(worker) = &manager.worker {
    worker.set_audio_volumes(audio_track_volumes)?;
  }
  Ok(())
}

#[tauri::command]
pub async fn stop_recording_preview_player(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let worker = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The recording preview player is unavailable".to_owned())?;
    if manager.session_id != Some(session_id) {
      return Ok(());
    }
    // The rest of the teardown - hiding the surface, releasing the sources -
    // has to stay under the lock so a later session cannot start on top of it;
    // only the playback worker's join moves off this thread.
    let worker = manager.take_worker();
    manager.stop();
    worker
  };
  if let Some(worker) = worker {
    cancel_off_thread(worker).await?;
  }
  Ok(())
}

pub fn stop_all(app: &AppHandle) {
  if let Some(state) = app.try_state::<RecordingPreviewPlayerState>() {
    if let Ok(mut manager) = state.0.lock() {
      manager.stop();
    }
  }
}
