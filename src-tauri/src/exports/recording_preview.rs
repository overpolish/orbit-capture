// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

struct RecordingPreviewSources {
  duration_ms: u64,
  path: PathBuf,
  tracks: Vec<RecordingAudioTrack>,
}

/// Prepares the lightweight waveform data used beside the native player.
#[tauri::command]
pub async fn get_recording_preview(
  app: AppHandle,
  artifact_id: u64,
) -> Result<media_preview::RecordingPreview, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let _preparing = state
      .recording_preview_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(preview) = state
      .recording_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .filter(|preview| preview.artifact_id == artifact_id)
      .cloned()
    {
      return Ok(preview);
    }

    let sources = recording_sources(&state, artifact_id)?;
    let preview = media_preview::prepare(
      artifact_id,
      &sources.path,
      sources.duration_ms,
      &sources.tracks,
    )?;
    ensure_current(&state, artifact_id)?;
    state
      .recording_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .replace(preview.clone());
    Ok(preview)
  })
  .await
  .map_err(|error| error.to_string())?
}

fn recording_sources(
  state: &ExportState,
  artifact_id: u64,
) -> Result<RecordingPreviewSources, String> {
  let artifact = state
    .recording
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(ExportArtifact::Recording {
    audio_tracks,
    duration_ms,
    id,
    path,
    ..
  }) = artifact.as_ref()
  else {
    return Err("There is no recording to preview".to_owned());
  };
  if *id != artifact_id {
    return Err("That recording is no longer waiting to be exported".to_owned());
  }
  Ok(RecordingPreviewSources {
    duration_ms: *duration_ms,
    path: path.clone(),
    tracks: audio_tracks.clone(),
  })
}

fn ensure_current(state: &ExportState, artifact_id: u64) -> Result<(), String> {
  let current = state
    .recording
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_ref()
    .is_some_and(
      |artifact| matches!(artifact, ExportArtifact::Recording { id, .. } if *id == artifact_id),
    );
  current
    .then_some(())
    .ok_or_else(|| "That recording is no longer waiting to be exported".to_owned())
}
