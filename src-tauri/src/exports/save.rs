// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// Moves a finished recording to where the user asked for it.
///
/// A rename when it can be, a copy when the destination is on another volume,
/// and never a re-encode: the movie was encoded once, while it was being
/// recorded, and encoding it again would cost minutes and quality for nothing.
pub(super) fn move_file(from: &Path, to: &Path) -> Result<(), String> {
  match std::fs::rename(from, to) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
      std::fs::copy(from, to).map_err(|error| error.to_string())?;
      // The copy is the save; failing to tidy up afterwards is not worth
      // telling the user their recording was not saved.
      let _ = std::fs::remove_file(from);
      Ok(())
    }
    Err(error) => Err(error.to_string()),
  }
}

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
/// just asked to keep. Either way exactly one file arrives, and the path
/// returned is the one that was written - never a name the caller assumed.
pub(super) fn save_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  remux: Option<media_preview::Remux>,
) -> Result<PathBuf, String> {
  let taken = |candidate: &Path| candidate.exists();
  if let Some(remux) = remux {
    let path = unique_path(directory, stem, RECORDING_EXTENSION, &taken);
    if remux(working, &path).is_ok() {
      // The stream copy is a copy: the working movie is only let go of once
      // its replacement is on disk under its final name.
      let _ = std::fs::remove_file(working);
      return Ok(path);
    }
  }

  let path = unique_path(directory, stem, delivered_extension(working, false), &taken);
  move_file(working, &path)?;

  Ok(path)
}

/// Saves a recording whose audio streams or layout differ from the source.
/// There is deliberately no `.mov` fallback here: keeping the source would
/// also keep tracks the user turned off, or fail to produce the requested
/// mixdown. The working recording remains untouched on every failure.
pub(super) fn save_selected_recording(
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
    media_preview::ExportRunResult::Completed => {
      let _ = std::fs::remove_file(working);
      Ok(Some(path))
    }
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

#[tauri::command]
pub async fn save_export(
  app: AppHandle,
  file_stem: String,
  enabled_stream_indices: Vec<usize>,
  collapse_audio: bool,
  compression: u8,
  resolution_scale_percent: u16,
) -> Result<Option<PathBuf>, String> {
  if compression > 4 {
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
  // write and lose the export window's source when a disk or mux operation
  // failed, which made Retry impossible even though the recording remained on
  // disk.
  let (result, artifact) = tauri::async_runtime::spawn_blocking(move || {
    let result = (|| -> Result<Option<PathBuf>, String> {
      std::fs::create_dir_all(&writing).map_err(|error| error.to_string())?;

      match &artifact {
        ExportArtifact::Screenshot { image, .. } => {
          let path = unique_path(&writing, &stem, SCREENSHOT_EXTENSION, &|candidate| {
            candidate.exists()
          });
          std::fs::write(&path, encode_png(image)?).map_err(|error| error.to_string())?;
          Ok(Some(path))
        }
        ExportArtifact::Recording {
          audio_tracks,
          id,
          path: working,
          source_scale_percent,
          ..
        } => {
          validate_resolution_scale(resolution_scale_percent, *source_scale_percent)?;
          let selection =
            track_selection::TrackSelection::new(audio_tracks, &enabled_stream_indices);
          let layout = if collapse_audio {
            track_selection::AudioLayout::Mixdown
          } else {
            track_selection::AudioLayout::SeparateTracks
          };

          if compression > 0
            || resolution_scale_percent < *source_scale_percent
            || selection.needs_processing(audio_tracks, layout)
          {
            let mut on_progress = |processed_ms| {
              let _ = progress_app.emit(
                EXPORT_PROGRESS_EVENT,
                ExportProgress {
                  artifact_id: *id,
                  processed_ms,
                },
              );
            };
            save_selected_recording(
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
            )
          } else {
            save_recording(working, &writing, &stem, media_preview::remuxer()).map(Some)
          }
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
