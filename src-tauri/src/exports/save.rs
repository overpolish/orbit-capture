// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

mod cursor;
mod location;
mod recording_file;

pub(super) use recording_file::{
  delivered_extension, save_primary_recording, save_recording_copy, save_selected_recording_copy,
  scale_percent, PrimaryRecordingSaveRequest,
};

#[cfg(test)]
pub(super) use recording_file::{save_recording, save_selected_recording};

#[tauri::command]
pub async fn save_export(
  app: AppHandle,
  file_stem: String,
  options: RecordingExportOptions,
) -> Result<Option<PathBuf>, String> {
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
    recording_output,
    screenshot_output,
  } = options;
  if compression > 4 || camera_compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }
  let stem =
    sanitize_file_stem(&file_stem).ok_or_else(|| "That file name cannot be used".to_owned())?;
  let directory =
    current_directory(&app).ok_or_else(|| "There is nowhere to save this".to_owned())?;
  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to save".to_owned())?;
  let artifact_id = match &artifact {
    ExportArtifact::Screenshot { id, .. } | ExportArtifact::Recording { id, .. } => *id,
  };
  let screenshot_preference =
    matches!(&artifact, ExportArtifact::Screenshot { .. }).then(|| screenshot_output.clone());
  let recording_preference =
    matches!(&artifact, ExportArtifact::Recording { .. }).then(|| recording_output.clone());
  let cancelled = Arc::new(AtomicBool::new(false));
  {
    let state = app.state::<ExportState>();
    let mut active = state
      .active_export
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if active.is_some() {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Err("Another export is already running".to_owned());
    }
    *active = Some(ActiveExportJob {
      artifact_id,
      cancelled: Arc::clone(&cancelled),
    });
  }

  let writing = directory.clone();
  let progress_app = app.clone();
  let job_cancellation = Arc::clone(&cancelled);
  // The artifact travels back with an error. Saving used to take it before the
  // write and lose its source on a disk or mux failure, making Retry impossible
  // even though the recording remained on disk.
  let (result, artifact) = tauri::async_runtime::spawn_blocking(move || {
    let result = (|| -> Result<Option<PathBuf>, String> {
      std::fs::create_dir_all(&writing).map_err(|error| error.to_string())?;

      match &artifact {
        ExportArtifact::Screenshot { image, .. } => {
          let path = unique_path(&writing, &stem, SCREENSHOT_EXTENSION, &|candidate| {
            candidate.exists()
          });
          let composed = compose_screenshot(image, &screenshot_output)?;
          std::fs::write(&path, encode_png(&composed)?).map_err(|error| error.to_string())?;
          Ok(Some(path))
        }
        ExportArtifact::Recording {
          audio_tracks,
          camera,
          cursor,
          duration_ms,
          height,
          id,
          path: working,
          primary_kind,
          source_scale_percent,
          width,
          ..
        } => {
          validate_primary_resolution_scale(
            resolution_scale_percent,
            *source_scale_percent,
            *primary_kind,
          )?;
          validate_camera_resolution_scale(camera_resolution_scale_percent)?;
          validate_camera_overlay(camera_overlay)?;
          let selection = track_selection::TrackSelection::with_volumes(
            audio_tracks,
            &enabled_stream_indices,
            &audio_track_volumes,
          )?;
          let layout = if collapse_audio {
            track_selection::AudioLayout::Mixdown
          } else {
            track_selection::AudioLayout::SeparateTracks
          };
          let baked_cursor = cursor
            .as_ref()
            .filter(|_| cursor_effects.bake)
            .map(|cursor| cursor.path.as_path());
          let primary_output = &recording_output.primary;
          let camera_output = &recording_output.camera;

          if !include_primary_video && !include_camera && enabled_stream_indices.is_empty() {
            return Err("Select at least one track to export".to_owned());
          }

          if *primary_kind == PrimaryRecordingKind::Audio {
            if include_primary_video || include_camera || bake_camera {
              return Err("This audio recording has no video track to export".to_owned());
            }
            return audio_save::save_audio(audio_save::AudioSaveRequest {
              app: &progress_app,
              cancelled: &job_cancellation,
              directory: &writing,
              duration_ms: *duration_ms,
              id: *id,
              layout,
              selected_any: !enabled_stream_indices.is_empty(),
              selection: &selection,
              stem: &stem,
              working,
            });
          }

          if !include_primary_video {
            if include_camera {
              let camera = camera
                .as_ref()
                .ok_or_else(|| "There is no camera track to export".to_owned())?;
              let saved = camera_save::save_camera_as_primary(
                working,
                camera,
                &writing,
                &stem,
                &selection,
                layout,
                *id,
                &progress_app,
                &job_cancellation,
                camera_compression,
                camera_resolution_scale_percent,
                camera_output,
              )?;
              if saved.is_some() {
                let _ = std::fs::remove_file(working);
                let _ = std::fs::remove_file(&camera.path);
                cursor::remove_working_file(cursor.as_ref());
              }
              return Ok(saved);
            }

            let saved = audio_save::save_audio(audio_save::AudioSaveRequest {
              app: &progress_app,
              cancelled: &job_cancellation,
              directory: &writing,
              duration_ms: *duration_ms,
              id: *id,
              layout,
              selected_any: !enabled_stream_indices.is_empty(),
              selection: &selection,
              stem: &stem,
              working,
            })?;
            if saved.is_some() {
              if let Some(camera) = camera {
                let _ = std::fs::remove_file(&camera.path);
              }
              cursor::remove_working_file(cursor.as_ref());
            }
            return Ok(saved);
          }

          if bake_camera {
            if !include_camera {
              return Err("Select the camera track before baking it in".to_owned());
            }
            let camera = camera
              .as_ref()
              .ok_or_else(|| "There is no camera recording to bake in".to_owned())?;
            let saved = camera_save::save_baked_recording(
              working,
              camera,
              &writing,
              &stem,
              &selection,
              layout,
              *id,
              *duration_ms,
              (*width, *height),
              camera_overlay,
              (compression, resolution_scale_percent, *source_scale_percent),
              baked_cursor.map(|cursor| (cursor, cursor_effects)),
              primary_output,
              &progress_app,
              &job_cancellation,
            )?;
            if saved.is_some() {
              let _ = std::fs::remove_file(working);
              let _ = std::fs::remove_file(&camera.path);
              cursor::remove_working_file(cursor.as_ref());
            }
            return Ok(saved);
          }

          let screen_progress_share = if include_camera && camera.is_some() {
            50.0
          } else {
            99.0
          };
          let saved = save_primary_recording(PrimaryRecordingSaveRequest {
            app: &progress_app,
            artifact_id: *id,
            audio_tracks,
            cancelled: &job_cancellation,
            compression,
            cursor: baked_cursor,
            cursor_effects,
            directory: &writing,
            duration_ms: *duration_ms,
            height: *height,
            layout,
            output: primary_output,
            progress_share: screen_progress_share,
            resolution_scale_percent,
            screen: working,
            selection: &selection,
            source_scale_percent: *source_scale_percent,
            stem: &stem,
            width: *width,
          })?;
          let Some(saved) = saved else {
            return Ok(None);
          };

          let next_phase = if include_camera && camera.is_some() {
            "camera"
          } else {
            "finalizing"
          };
          let _ = progress_app.emit(
            EXPORT_PROGRESS_EVENT,
            ExportProgress {
              artifact_id: *id,
              phase: next_phase,
              progress_percent: screen_progress_share,
            },
          );

          let mut saved_camera = None;
          if include_camera {
            let camera = camera
              .as_ref()
              .ok_or_else(|| "There is no camera track to export".to_owned())?;
            // The camera remains a separate file, with its own compression
            // and resolution choice. It shares the transaction and progress
            // timeline with the screen recording but not its encode settings.
            let camera_path = camera_save::save_camera_copy(
              camera,
              &writing,
              &stem,
              *id,
              &progress_app,
              &job_cancellation,
              screen_progress_share,
              camera_compression,
              camera_resolution_scale_percent,
              camera_output,
            )
            .inspect_err(|_| {
              let _ = std::fs::remove_file(&saved);
            })?;
            let Some(camera_path) = camera_path else {
              let _ = std::fs::remove_file(&saved);
              return Ok(None);
            };
            saved_camera = Some(camera_path);
          }
          let _ = progress_app.emit(
            EXPORT_PROGRESS_EVENT,
            ExportProgress {
              artifact_id: *id,
              phase: "finalizing",
              progress_percent: 99.0,
            },
          );
          if !saved.is_file() || saved_camera.as_ref().is_some_and(|path| !path.is_file()) {
            let _ = std::fs::remove_file(&saved);
            if let Some(path) = saved_camera {
              let _ = std::fs::remove_file(path);
            }
            return Err("The exported recording did not finish publishing".to_owned());
          }
          let _ = std::fs::remove_file(working);
          if !include_camera {
            if let Some(camera) = camera {
              let _ = std::fs::remove_file(&camera.path);
            }
          }
          cursor::remove_working_file(cursor.as_ref());
          Ok(Some(saved))
        }
      }
    })();

    (result, artifact)
  })
  .await
  .map_err(|error| error.to_string())?;

  {
    let state = app.state::<ExportState>();
    let mut active = state
      .active_export
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if active
      .as_ref()
      .is_some_and(|job| job.artifact_id == artifact_id)
    {
      active.take();
    }
  }

  let path = match result {
    Ok(Some(path)) => path,
    Ok(None) => {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Ok(None);
    }
    Err(error) => {
      *app
        .state::<ExportState>()
        .artifact
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(artifact);
      emit_snapshot(&app);
      return Err(error);
    }
  };

  set_export_directory(app.clone(), directory)?;
  remember_completed_export(
    &app,
    cursor_effects,
    recording_preference,
    screenshot_preference,
  );
  // Saving is transactional: keep the native player alive while the artifact
  // may still be restored by Cancel or an export error, then retire it only
  // once the finished files have been published.
  artifact::clear_recording_preview(&app);
  artifact::clear_cached_previews(&app);
  let _ = window::hide(&app);
  emit_snapshot(&app);

  if crate::settings::current(&app).open_location_after_export {
    if let Err(error) = location::open_containing_folder(&path) {
      eprintln!("Could not open the export location: {error}");
    }
  }

  Ok(Some(path))
}
