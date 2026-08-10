// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::save::{save_recording_copy, save_selected_recording_copy};
use super::*;

pub(super) fn validate_camera_overlay(settings: CameraOverlaySettings) -> Result<(), String> {
  let values = [
    settings.camera_x_percent,
    settings.camera_y_percent,
    settings.camera_width_percent,
    settings.frame_height_percent,
    settings.frame_width_percent,
    settings.frame_x_percent,
    settings.frame_y_percent,
    settings.radius_percent,
  ];
  if values.iter().any(|value| !value.is_finite())
    || !(0.0..=100.0).contains(&settings.camera_x_percent)
    || !(0.0..=100.0).contains(&settings.camera_y_percent)
    || !(3.0..=200.0).contains(&settings.camera_width_percent)
    || !(3.0..=100.0).contains(&settings.frame_height_percent)
    || !(3.0..=100.0).contains(&settings.frame_width_percent)
    || !(0.0..=100.0).contains(&settings.frame_x_percent)
    || !(0.0..=100.0).contains(&settings.frame_y_percent)
    || settings.frame_x_percent + settings.frame_width_percent > 100.000_001
    || settings.frame_y_percent + settings.frame_height_percent > 100.000_001
    || !(0.0..=50.0).contains(&settings.radius_percent)
  {
    return Err("The camera overlay settings are not valid".to_owned());
  }
  Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_progress(
  app: &AppHandle,
  artifact_id: u64,
  phase: &'static str,
  processed_ms: u64,
  duration_ms: u64,
  start: f64,
  share: f64,
) {
  let progress_percent = if duration_ms == 0 {
    start
  } else {
    start + (processed_ms as f64 / duration_ms as f64 * share).min(share)
  };
  let _ = app.emit(
    EXPORT_PROGRESS_EVENT,
    ExportProgress {
      artifact_id,
      phase,
      progress_percent,
    },
  );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn save_baked_recording(
  screen: &Path,
  camera: &RecordingCamera,
  directory: &Path,
  stem: &str,
  selection: &track_selection::TrackSelection,
  layout: track_selection::AudioLayout,
  artifact_id: u64,
  duration_ms: u64,
  screen_size: (u32, u32),
  overlay: CameraOverlaySettings,
  video_settings: (u8, u16, u16),
  progress_app: &AppHandle,
  cancelled: &AtomicBool,
) -> Result<Option<PathBuf>, String> {
  let mut on_progress = |processed_ms| {
    emit_progress(
      progress_app,
      artifact_id,
      "recording",
      processed_ms,
      duration_ms,
      0.0,
      99.0,
    );
  };
  let exporter = media_preview::baked_recording_exporter()
    .ok_or_else(|| "FFmpeg is required to bake in the camera recording".to_owned())?;
  let path = unique_path(directory, stem, RECORDING_EXTENSION, &|candidate| {
    candidate.exists()
  });
  match exporter(
    screen,
    &camera.path,
    &path,
    selection,
    layout,
    media_preview::BakedVideoExportOptions {
      camera_height: camera.height,
      camera_width: camera.width,
      overlay,
      screen_height: screen_size.1,
      screen_width: screen_size.0,
      video: media_preview::VideoExportOptions {
        compression: video_settings.0,
        resolution_scale_percent: video_settings.1,
        source_scale_percent: video_settings.2,
      },
    },
    cancelled,
    &mut on_progress,
  )? {
    media_preview::ExportRunResult::Completed => {
      emit_progress(
        progress_app,
        artifact_id,
        "finalizing",
        duration_ms,
        duration_ms,
        0.0,
        99.0,
      );
      if !path.is_file() {
        let _ = std::fs::remove_file(&path);
        return Err("The exported recording did not finish publishing".to_owned());
      }
      let _ = std::fs::remove_file(screen);
      let _ = std::fs::remove_file(&camera.path);
      Ok(Some(path))
    }
    media_preview::ExportRunResult::Cancelled => Ok(None),
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn save_camera_copy(
  camera: &RecordingCamera,
  directory: &Path,
  stem: &str,
  artifact_id: u64,
  progress_app: &AppHandle,
  cancelled: &AtomicBool,
  progress_start: f64,
  compression: u8,
  resolution_scale_percent: u16,
) -> Result<Option<PathBuf>, String> {
  let camera_stem = format!("{stem} Camera");
  let empty_selection = track_selection::TrackSelection::default();
  let mut on_progress = |processed_ms| {
    emit_progress(
      progress_app,
      artifact_id,
      "camera",
      processed_ms,
      camera.duration_ms,
      progress_start,
      99.0 - progress_start,
    );
  };

  let exporter = media_preview::selected_recording_exporter();
  if exporter.is_none() && (compression > 0 || resolution_scale_percent < 100) {
    return Err("FFmpeg is required to compress the camera recording".to_owned());
  };
  let saved = if let Some(exporter) = exporter {
    save_selected_recording_copy(
      &camera.path,
      directory,
      &camera_stem,
      &empty_selection,
      track_selection::AudioLayout::SeparateTracks,
      media_preview::ExportRunOptions {
        cancelled,
        on_progress: &mut on_progress,
        video: media_preview::VideoExportOptions {
          compression,
          resolution_scale_percent,
          source_scale_percent: 100,
        },
      },
      Some(exporter),
    )?
  } else {
    Some(save_recording_copy(
      &camera.path,
      directory,
      &camera_stem,
      None,
    )?)
  };
  if saved.is_some() {
    let _ = std::fs::remove_file(&camera.path);
  }
  Ok(saved)
}
