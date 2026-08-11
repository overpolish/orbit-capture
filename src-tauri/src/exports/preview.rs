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

/// A sampled estimate of the file the current export choices would produce.
/// Video is the expensive unknown; selected AAC sizes are derived from their
/// actual configured bitrates and added after the sample is extrapolated.
#[tauri::command]
pub async fn estimate_recording_export(
  app: AppHandle,
  artifact_id: u64,
  options: RecordingExportOptions,
) -> Result<u64, String> {
  let RecordingExportOptions {
    audio_track_volumes,
    bake_camera,
    camera_compression,
    camera_overlay,
    camera_resolution_scale_percent,
    collapse_audio,
    compression,
    cursor_effects,
    enabled_stream_indices,
    include_camera,
    include_primary_video,
    resolution_scale_percent,
    screenshot_radius_percent: _,
  } = options;
  if compression > 4 || camera_compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }
  validate_camera_overlay(camera_overlay)?;

  tauri::async_runtime::spawn_blocking(move || {
    let state = app.state::<ExportState>();
    let (
      path,
      tracks,
      duration_ms,
      original_size,
      camera,
      has_video,
      width,
      height,
      primary_kind,
      source_scale_percent,
      has_cursor,
    ) = {
      let artifact = state
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let Some(ExportArtifact::Recording {
        audio_tracks,
        camera,
        cursor,
        duration_ms,
        height,
        id,
        path,
        primary_kind,
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
        camera.clone(),
        *width > 0 && *height > 0,
        *width,
        *height,
        *primary_kind,
        *source_scale_percent,
        cursor.is_some(),
      )
    };
    if bake_camera && (!include_primary_video || !include_camera || camera.is_none()) {
      return Err("There is no camera recording to bake in".to_owned());
    }
    let bake_cursor = cursor_effects.bake && include_primary_video && has_cursor;
    if !include_primary_video && !include_camera && enabled_stream_indices.is_empty() {
      return Err("Select at least one track to export".to_owned());
    }
    validate_primary_resolution_scale(
      resolution_scale_percent,
      source_scale_percent,
      primary_kind,
    )?;
    validate_camera_resolution_scale(camera_resolution_scale_percent)?;

    let selection = track_selection::TrackSelection::with_volumes(
      &tracks,
      &enabled_stream_indices,
      &audio_track_volumes,
    )?;
    let layout = if collapse_audio {
      track_selection::AudioLayout::Mixdown
    } else {
      track_selection::AudioLayout::SeparateTracks
    };
    let selected_audio = selection.estimated_audio_bytes(&tracks, layout, duration_ms);

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

    let _preparing = state
      .compression_estimate_preparation
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let estimate_compression = if (bake_camera || bake_cursor) && compression == 0 {
      // Original means no intentional quality reduction. Composition still
      // requires an encode, so it uses the same high-quality step as High
      // rather than pretending the source can be stream-copied.
      1
    } else {
      compression
    };
    let key = (
      artifact_id,
      u8::from(bake_camera) * 2 + u8::from(bake_cursor) * 4,
      estimate_compression,
      resolution_scale_percent,
    );
    let cached = state
      .compression_estimates
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .get(&key)
      .copied();
    let screen_video = match cached {
      _ if !include_primary_video => 0,
      Some(bytes) => bytes,
      None if !has_video => 0,
      None if bake_cursor && !bake_camera => cursor_export::estimated_video_bytes(
        width,
        height,
        duration_ms,
        media_preview::VideoExportOptions {
          compression: estimate_compression,
          resolution_scale_percent,
          source_scale_percent,
        },
      ),
      None
        if !bake_camera
          && !bake_cursor
          && compression == 0
          && resolution_scale_percent == source_scale_percent =>
      {
        original_size.saturating_sub(original_audio)
      }
      None => {
        let bytes = media_preview::estimate_compressed_video_bytes(
          &path,
          duration_ms,
          estimate_compression,
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

    let camera_video = if !include_camera {
      0
    } else if bake_camera {
      // Baking necessarily re-encodes the screen. The screen sample remains
      // the useful predictor; a small allowance covers motion in the overlay
      // without counting the camera as a second deliverable file.
      0
    } else if let Some(camera) = camera {
      let key = (
        artifact_id,
        1,
        camera_compression,
        camera_resolution_scale_percent,
      );
      let cached = state
        .compression_estimates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .copied();
      match cached {
        Some(bytes) => bytes,
        None if camera_compression == 0 && camera_resolution_scale_percent == 100 => {
          camera.original_size_bytes
        }
        None => {
          let bytes = media_preview::estimate_compressed_video_bytes(
            &camera.path,
            camera.duration_ms,
            camera_compression,
            100,
            camera_resolution_scale_percent,
          )?;
          state
            .compression_estimates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, bytes);
          bytes
        }
      }
    } else {
      0
    };

    // MP4's tables are small but real. Half a percent plus its fixed headers
    // keeps the estimate honest without pretending CRF can predict exact size.
    let screen_video = if bake_camera {
      screen_video.saturating_add(screen_video / 12)
    } else {
      screen_video
    };
    let media = screen_video
      .saturating_add(camera_video)
      .saturating_add(selected_audio);
    Ok(media.saturating_add(media / 200).saturating_add(4_096))
  })
  .await
  .map_err(|error| error.to_string())?
}
