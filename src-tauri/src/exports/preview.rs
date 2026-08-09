// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[tauri::command]
pub fn get_export_snapshot(app: AppHandle) -> ExportSnapshot {
  snapshot(&app)
}

/// The thumbnail by default; the full-resolution capture only once something
/// actually needs it, and cached from then on.
#[tauri::command]
pub async fn get_export_preview(app: AppHandle, full: bool) -> Result<Response, String> {
  let bytes = tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let missing = || "There is nothing waiting to be exported".to_owned();

    if !full {
      return state
        .preview
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .ok_or_else(missing);
    }

    if let Some(cached) = state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .clone()
    {
      return Ok(cached);
    }

    let encoded = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      // A recording has no full-resolution still to zoom into, so the poster
      // it was presented with is all there is.
      let Some(ExportArtifact::Screenshot { image, .. }) = artifact.as_ref() else {
        return Err(missing());
      };
      full_preview_png(image)?
    };
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(encoded.clone());

    Ok(encoded)
  })
  .await
  .map_err(|error| error.to_string())??;

  Ok(Response::new(bytes))
}

/// Prepares independently playable audio tracks and compact waveform data for
/// the recording currently shown in the export window.
///
/// The preparation mutex is intentional: React may mount an effect twice in a
/// development build, and two FFmpeg processes must never race to replace the
/// same preview files. The second caller waits, then takes the cached result.
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

    let (path, duration_ms, tracks) = {
      let artifact = state
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
      (path.clone(), *duration_ms, audio_tracks.clone())
    };

    let preview = media_preview::prepare(artifact_id, &path, duration_ms, &tracks)?;
    let is_current = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .is_some_and(
        |artifact| matches!(artifact, ExportArtifact::Recording { id, .. } if *id == artifact_id),
      );
    if !is_current {
      return Err("That recording is no longer waiting to be exported".to_owned());
    }

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

/// The single file the export window should play for a set of enabled tracks.
///
/// The window plays one `<video>` and nothing else. Playing the recording
/// alongside separate `<audio>` elements and correcting their drift was tried
/// first and cannot be made to work: setting `currentTime` on a media element
/// is a seek, a seek silences it until its decoder catches up, and the video
/// never stops advancing meanwhile - so each correction arrives before the
/// last one produced any sound. Handing the element one already-muxed file
/// makes the whole class of problem disappear, because there is only one clock.
///
/// What comes back is a *playback* file. See [`track_selection`] for why that
/// is not what saving the recording produces.
#[tauri::command]
pub async fn get_recording_preview_mix(
  app: AppHandle,
  artifact_id: u64,
  enabled_stream_indices: Vec<usize>,
) -> Result<PathBuf, String> {
  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    // Held for the same reason the track preparation is: a debounced toggle
    // and a mount effect can ask at once, and two FFmpeg processes must never
    // race to write the same file. The second caller waits and finds it built.
    let _preparing = state
      .preview_mix_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());

    let (path, tracks) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
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
      (path.clone(), audio_tracks.clone())
    };

    let selection = track_selection::TrackSelection::new(&tracks, &enabled_stream_indices);
    let mixed = {
      let mut mixes = state
        .preview_mixes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      media_preview::preview_mix(&mut mixes, artifact_id, &path, &tracks, &selection)?
    };

    // The recording can be discarded while FFmpeg is still working, and the
    // cleanup that ran then knew nothing about a file that did not yet exist.
    let is_current = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref()
      .is_some_and(
        |artifact| matches!(artifact, ExportArtifact::Recording { id, .. } if *id == artifact_id),
      );
    if !is_current {
      state
        .preview_mixes
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .cleanup();
      return Err("That recording is no longer waiting to be exported".to_owned());
    }

    Ok(mixed)
  })
  .await
  .map_err(|error| error.to_string())?
}

/// A sampled estimate of the file the current export choices would produce.
/// Video is the expensive unknown; selected AAC sizes are derived from their
/// actual configured bitrates and added after the sample is extrapolated.
#[tauri::command]
pub async fn estimate_recording_export(
  app: AppHandle,
  artifact_id: u64,
  enabled_stream_indices: Vec<usize>,
  collapse_audio: bool,
  compression: u8,
  resolution_scale_percent: u16,
) -> Result<u64, String> {
  if compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }

  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let (path, tracks, duration_ms, original_size, has_video, source_scale_percent) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
        duration_ms,
        height,
        id,
        path,
        source_scale_percent,
        width,
        ..
      }) = artifact.as_ref()
      else {
        return Err("There is no recording to estimate".to_owned());
      };
      if *id != artifact_id {
        return Err("That recording is no longer waiting to be exported".to_owned());
      }
      (
        path.clone(),
        audio_tracks.clone(),
        *duration_ms,
        std::fs::metadata(path).map_or(0, |metadata| metadata.len()),
        *width > 0 && *height > 0,
        *source_scale_percent,
      )
    };
    validate_resolution_scale(resolution_scale_percent, source_scale_percent)?;

    let selection = track_selection::TrackSelection::new(&tracks, &enabled_stream_indices);
    let layout = if collapse_audio {
      track_selection::AudioLayout::Mixdown
    } else {
      track_selection::AudioLayout::SeparateTracks
    };
    let selected_audio = selection.estimated_audio_bytes(&tracks, layout, duration_ms);

    // The original route only remuxes, so the source size minus its known AAC
    // streams is the best video measurement available without parsing it.
    if compression == 0 && resolution_scale_percent == source_scale_percent {
      let all_indices = tracks
        .iter()
        .map(|track| track.stream_index)
        .collect::<Vec<_>>();
      let all = track_selection::TrackSelection::new(&tracks, &all_indices);
      let original_audio = all.estimated_audio_bytes(
        &tracks,
        track_selection::AudioLayout::SeparateTracks,
        duration_ms,
      );
      return Ok(
        original_size
          .saturating_sub(original_audio)
          .saturating_add(selected_audio),
      );
    }

    let _preparing = state
      .compression_estimate_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = (artifact_id, compression, resolution_scale_percent);
    let cached = state
      .compression_estimates
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .get(&key)
      .copied();
    let video = match cached {
      Some(bytes) => bytes,
      None if !has_video => 0,
      None => {
        let bytes = media_preview::estimate_compressed_video_bytes(
          &path,
          duration_ms,
          compression,
          source_scale_percent,
          resolution_scale_percent,
        )?;
        state
          .compression_estimates
          .lock()
          .unwrap_or_else(|poisoned| poisoned.into_inner())
          .insert(key, bytes);
        bytes
      }
    };

    // MP4's tables are small but real. Half a percent plus its fixed headers
    // keeps the estimate honest without pretending CRF can predict exact size.
    let media = video.saturating_add(selected_audio);
    Ok(media.saturating_add(media / 200).saturating_add(4_096))
  })
  .await
  .map_err(|error| error.to_string())?
}
