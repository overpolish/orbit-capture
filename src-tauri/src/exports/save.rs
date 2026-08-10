// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The extension the working recording will actually be saved under.
///
/// `.mp4` whenever it can be remuxed into one, because that is the file
/// everything opens and nobody should have to know what the app records to.
/// Without FFmpeg the QuickTime movie is handed over as it is, under its own
/// name: renaming a `.mov` to `.mp4` would produce a file that lies about
/// itself, and some players trust the name over the bytes.
///
/// Falls back to the working file's own extension rather than a constant, so
/// a recording recovered from an older version - which really is an `.mp4` -
/// is described correctly too.
pub(super) fn delivered_extension(working: &Path, can_remux: bool) -> &str {
  if can_remux {
    return RECORDING_EXTENSION;
  }

  working
    .extension()
    .and_then(|extension| extension.to_str())
    .unwrap_or(WORKING_RECORDING_EXTENSION)
}

/// Puts a finished recording where the user asked for it, as an .mp4 if that
/// is possible and honestly as what it is if it is not.
///
/// The remux is attempted first and its failure is not an error: FFmpeg being
/// absent, or refusing the file, is no reason to lose a recording the user
/// just asked to keep. The path returned is the one that was written - never
/// a name the caller assumed.
#[cfg(test)]
pub(super) fn save_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  remux: Option<media_preview::Remux>,
) -> Result<PathBuf, String> {
  let path = save_recording_copy(working, directory, stem, remux)?;
  let _ = std::fs::remove_file(working);
  Ok(path)
}

/// Writes a deliverable without consuming its working movie. Multi-file
/// camera exports use this transactionally: both outputs must land before
/// either recoverable source is removed.
pub(super) fn save_recording_copy(
  working: &Path,
  directory: &Path,
  stem: &str,
  remux: Option<media_preview::Remux>,
) -> Result<PathBuf, String> {
  let taken = |candidate: &Path| candidate.exists();
  if let Some(remux) = remux {
    let path = unique_path(directory, stem, RECORDING_EXTENSION, &taken);
    if remux(working, &path).is_ok() {
      return Ok(path);
    }
  }

  let path = unique_path(directory, stem, delivered_extension(working, false), &taken);
  std::fs::copy(working, &path).map_err(|error| error.to_string())?;

  Ok(path)
}

/// Saves a recording whose audio streams or layout differ from the source.
/// There is deliberately no `.mov` fallback here: keeping the source would
/// also keep tracks the user turned off, or fail to produce the requested
/// mixdown. The working recording remains untouched on every failure.
#[cfg(test)]
pub(super) fn save_selected_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  selection: &track_selection::TrackSelection,
  layout: track_selection::AudioLayout,
  run: media_preview::ExportRunOptions<'_>,
  exporter: Option<media_preview::SelectedRecordingExport>,
) -> Result<Option<PathBuf>, String> {
  let path =
    save_selected_recording_copy(working, directory, stem, selection, layout, run, exporter)?;
  if path.is_some() {
    let _ = std::fs::remove_file(working);
  }
  Ok(path)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn save_selected_recording_copy(
  working: &Path,
  directory: &Path,
  stem: &str,
  selection: &track_selection::TrackSelection,
  layout: track_selection::AudioLayout,
  run: media_preview::ExportRunOptions<'_>,
  exporter: Option<media_preview::SelectedRecordingExport>,
) -> Result<Option<PathBuf>, String> {
  let exporter = exporter.ok_or_else(|| {
    "FFmpeg is required to compress or change which audio tracks are exported".to_owned()
  })?;
  let path = unique_path(directory, stem, RECORDING_EXTENSION, &|candidate| {
    candidate.exists()
  });
  match exporter(working, &path, selection, layout, run)? {
    media_preview::ExportRunResult::Completed => Ok(Some(path)),
    media_preview::ExportRunResult::Cancelled => Ok(None),
  }
}

pub(super) fn scale_percent(scale_factor: f32) -> u16 {
  (scale_factor.max(1.0) * 100.0).round().clamp(100.0, 400.0) as u16
}

pub(super) fn validate_resolution_scale(selected: u16, source: u16) -> Result<(), String> {
  if selected < 100 || selected > source {
    return Err("The selected output resolution is not available for this recording".to_owned());
  }

  Ok(())
}

pub(super) fn validate_camera_resolution_scale(selected: u16) -> Result<(), String> {
  if ![50, 75, 100].contains(&selected) {
    return Err("The selected camera resolution is not available".to_owned());
  }

  Ok(())
}

#[tauri::command]
pub async fn save_export(
  app: AppHandle,
  file_stem: String,
  options: RecordingExportOptions,
) -> Result<Option<PathBuf>, String> {
  let RecordingExportOptions {
    bake_camera,
    camera_compression,
    camera_overlay,
    camera_resolution_scale_percent,
    collapse_audio,
    compression,
    enabled_stream_indices,
    resolution_scale_percent,
    screenshot_radius_percent,
  } = options;
  if compression > 4 || camera_compression > 4 {
    return Err("Compression must be between 0 and 4".to_owned());
  }
  let screenshot_radius_percent = remember_screenshot_radius(&app, screenshot_radius_percent)?;
  let stem =
    sanitize_file_stem(&file_stem).ok_or_else(|| "That file name cannot be used".to_owned())?;
  let directory =
    current_directory(&app).ok_or_else(|| "There is nowhere to save this".to_owned())?;
  let artifact = take_artifact(&app).ok_or_else(|| "There is nothing to save".to_owned())?;
  let artifact_id = match &artifact {
    ExportArtifact::Screenshot { id, .. } | ExportArtifact::Recording { id, .. } => *id,
  };
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
          let rounded = rounded_corners(image, screenshot_radius_percent);
          std::fs::write(&path, encode_png(&rounded)?).map_err(|error| error.to_string())?;
          Ok(Some(path))
        }
        ExportArtifact::Recording {
          audio_tracks,
          camera,
          duration_ms,
          height,
          id,
          path: working,
          source_scale_percent,
          width,
          ..
        } => {
          validate_resolution_scale(resolution_scale_percent, *source_scale_percent)?;
          validate_camera_resolution_scale(camera_resolution_scale_percent)?;
          validate_camera_overlay(camera_overlay)?;
          let selection =
            track_selection::TrackSelection::new(audio_tracks, &enabled_stream_indices);
          let layout = if collapse_audio {
            track_selection::AudioLayout::Mixdown
          } else {
            track_selection::AudioLayout::SeparateTracks
          };

          if bake_camera {
            let camera = camera
              .as_ref()
              .ok_or_else(|| "There is no camera recording to bake in".to_owned())?;
            return camera_save::save_baked_recording(
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
              &progress_app,
              &job_cancellation,
            );
          }

          let screen_progress_share = if camera.is_some() { 50.0 } else { 99.0 };
          let saved = if compression > 0
            || resolution_scale_percent < *source_scale_percent
            || selection.needs_processing(audio_tracks, layout)
          {
            let mut on_progress = |processed_ms| {
              camera_save::emit_progress(
                &progress_app,
                *id,
                "recording",
                processed_ms,
                *duration_ms,
                0.0,
                screen_progress_share,
              );
            };
            save_selected_recording_copy(
              working,
              &writing,
              &stem,
              &selection,
              layout,
              media_preview::ExportRunOptions {
                cancelled: &job_cancellation,
                on_progress: &mut on_progress,
                video: media_preview::VideoExportOptions {
                  compression,
                  resolution_scale_percent,
                  source_scale_percent: *source_scale_percent,
                },
              },
              media_preview::selected_recording_exporter(),
            )?
          } else {
            Some(save_recording_copy(
              working,
              &writing,
              &stem,
              media_preview::remuxer(),
            )?)
          };
          let Some(saved) = saved else {
            return Ok(None);
          };

          let next_phase = if camera.is_some() {
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
          if let Some(camera) = camera {
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

  // Only remembered once a save actually lands there.
  set_export_directory(app.clone(), directory)?;
  let _ = window::hide(&app);
  emit_snapshot(&app);

  Ok(Some(path))
}
