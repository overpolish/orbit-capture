// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{ipc::Channel, AppHandle, Manager};

use super::*;

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
  let sources = sources(&app, artifact_id, settings.cursor_effects)?;
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
  manager.latest_seek_request = 0;
  manager.position_ms = 0;
  manager.sources = Some(sources);
  manager.session_id = Some(session_id);
  manager.restart(PlaybackMode::Still)?;
  Ok(info)
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
    manager.restart(PlaybackMode::Still)?;
  }
  Ok(())
}

#[tauri::command]
pub fn play_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  manager.is_playing = true;
  manager.restart(PlaybackMode::Playing)
}

#[tauri::command]
pub fn pause_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  manager.is_playing = false;
  manager.cancel_worker();
  if let Some(channel) = &manager.event_channel {
    let _ = channel.send(RecordingPreviewPlayerEvent::Paused {
      position_ms: manager.position_ms,
    });
  }
  Ok(())
}

#[tauri::command]
pub fn request_recording_preview_full_resolution(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if manager.is_playing {
    return Ok(());
  }
  manager.cancel_worker();
  #[cfg(target_os = "macos")]
  if let Some(decoder) = &manager.still_decoder {
    return decoder.seek(manager.position_ms, manager.latest_seek_request, true);
  }
  let mut sources = manager
    .sources
    .clone()
    .ok_or_else(|| "The recording preview player is not open".to_owned())?;
  sources.playback_layout.clone_from(&sources.layout);
  let frame_channel = manager
    .frame_channel
    .clone()
    .ok_or_else(|| "The recording preview frame channel is unavailable".to_owned())?;
  let event_channel = manager
    .event_channel
    .clone()
    .ok_or_else(|| "The recording preview event channel is unavailable".to_owned())?;
  manager.worker = Some(PreviewPlayerWorker::spawn(
    sources,
    PreviewAudioSettings {
      audio_track_volumes: manager.audio_volumes.clone(),
      enabled_stream_indices: manager.audio_indices.clone(),
    },
    manager.position_ms,
    manager.latest_seek_request,
    PlaybackMode::Still,
    frame_channel,
    event_channel,
  )?);
  Ok(())
}

#[tauri::command]
pub fn seek_recording_preview(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  position_ms: u64,
  request_id: u64,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if request_id < manager.latest_seek_request {
    return Ok(());
  }
  manager.latest_seek_request = request_id;
  manager.cancel_worker();
  let duration_ms = manager
    .sources
    .as_ref()
    .map_or(0, |value| value.duration_ms);
  manager.position_ms = position_ms.min(duration_ms.saturating_sub(1));
  manager.is_playing = false;
  manager.restart(PlaybackMode::Still)
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
pub fn stop_recording_preview_player(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  if manager.session_id == Some(session_id) {
    manager.stop();
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
