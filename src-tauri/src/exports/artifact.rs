// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) fn snapshot(app: &AppHandle) -> ExportSnapshot {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .as_ref()
    .map(|artifact| match artifact {
      ExportArtifact::Screenshot {
        id,
        image,
        suggested_file_stem,
      } => ExportArtifactSnapshot::Screenshot {
        id: *id,
        suggested_file_stem: suggested_file_stem.clone(),
        extension: SCREENSHOT_EXTENSION.to_owned(),
        width: image.width,
        height: image.height,
      },
      ExportArtifact::Recording {
        audio_tracks,
        camera,
        duration_ms,
        height,
        id,
        path,
        primary_kind,
        source_scale_percent,
        suggested_file_stem,
        width,
      } => ExportArtifactSnapshot::Recording {
        audio_tracks: audio_tracks.clone(),
        camera: camera.clone(),
        can_compress: *primary_kind != PrimaryRecordingKind::Audio
          && media_preview::supports_compression(),
        id: *id,
        suggested_file_stem: suggested_file_stem.clone(),
        extension: if *primary_kind == PrimaryRecordingKind::Audio {
          AUDIO_EXTENSION.to_owned()
        } else {
          delivered_extension(path, media_preview::remuxer().is_some()).to_owned()
        },
        width: *width,
        height: *height,
        duration_ms: *duration_ms,
        original_size_bytes: std::fs::metadata(path).map_or(0, |metadata| metadata.len())
          + camera
            .as_ref()
            .and_then(|camera| std::fs::metadata(&camera.path).ok())
            .map_or(0, |metadata| metadata.len()),
        path: path.clone(),
        primary_kind: *primary_kind,
        source_scale_percent: *source_scale_percent,
      },
    });

  let screenshot_radius_percent = *state
    .screenshot_radius_percent
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());

  ExportSnapshot {
    artifact,
    directory: current_directory(app),
    screenshot_radius_percent,
  }
}

pub(super) fn emit_snapshot(app: &AppHandle) {
  let _ = app.emit(EXPORT_CHANGED_EVENT, snapshot(app));
}

/// Shrinks the capture to something worth sending over IPC.
pub(super) fn preview_png(image: &CapturedImage) -> Option<Vec<u8>> {
  let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())?;
  let scale = f64::from(PREVIEW_MAX_EDGE) / f64::from(image.width.max(image.height));
  let (width, height) = if scale >= 1.0 {
    (image.width, image.height)
  } else {
    (
      ((f64::from(image.width) * scale).round() as u32).max(1),
      ((f64::from(image.height) * scale).round() as u32).max(1),
    )
  };

  let thumbnail = image::DynamicImage::ImageRgba8(buffer).thumbnail(width, height);
  let mut png = Vec::new();
  thumbnail
    .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
    .ok()?;

  Some(png)
}

/// The capture at full resolution, for zooming into. Encoded losslessly and
/// quickly - this is for looking at, not for keeping, so the slow quantizing
/// encoder that produces the saved file would be the wrong trade here.
pub(super) fn full_preview_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  PngEncoder::new_with_quality(
    std::io::Cursor::new(&mut png),
    CompressionType::Fast,
    FilterType::Sub,
  )
  .write_image(
    &image.rgba,
    image.width,
    image.height,
    ExtendedColorType::Rgba8,
  )
  .map_err(|error| error.to_string())?;

  Ok(png)
}

/// Deletes the working file behind a recording that will not be saved.
///
/// A screenshot lives in memory and needs nothing; a recording is a file, and
/// every path that lets go of one without saving it comes through here.
pub(super) fn delete_working_file(artifact: &ExportArtifact) {
  if let ExportArtifact::Recording { camera, path, .. } = artifact {
    let _ = std::fs::remove_file(path);
    if let Some(camera) = camera {
      let _ = std::fs::remove_file(&camera.path);
    }
  }
}

/// Removes everything built for the artifact that is going away.
///
/// Every path that lets go of a recording - discarding it, replacing it with a
/// new capture, saving it - comes through here, so no derivative outlives the
/// artifact it was made from.
pub(super) fn clear_recording_preview(app: &AppHandle) {
  super::recording_preview_player::stop_all(app);
  let state = app.state::<ExportState>();
  state
    .recording_preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();
  state
    .compression_estimates
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clear();
}

/// The next artifact identity. Two consecutive captures are otherwise
/// indistinguishable, and the window needs to tell them apart.
pub(super) fn next_id(app: &AppHandle) -> u64 {
  app
    .state::<ExportState>()
    .generation
    .fetch_add(1, Ordering::SeqCst)
    .wrapping_add(1)
}

/// Puts an artifact in front of the user, replacing whatever was waiting.
pub(super) fn present(
  app: &AppHandle,
  artifact: ExportArtifact,
  preview: Option<Vec<u8>>,
) -> Result<(), String> {
  clear_recording_preview(app);
  {
    let state = app.state::<ExportState>();
    *state
      .preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = preview;
    *state
      .full_preview
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let replaced = state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .replace(artifact);
    // Taking a screenshot while a recording waits to be saved abandons the
    // recording, and an abandoned recording must not outlive its window.
    if let Some(replaced) = replaced {
      delete_working_file(&replaced);
    }
  }

  window::show(app).map_err(|error| error.to_string())?;
  // Once an artifact is safely in front of the user, the capture controls
  // have finished their job. Keeping this at the shared presentation boundary
  // gives screenshots and recordings the same handoff without affecting
  // clipboard-only screenshots, which never open the export window.
  let _ = crate::windows::hide_recording_ui(app.clone());
  emit_snapshot(app);

  Ok(())
}

/// Hands a freshly captured still to the export window.
pub fn present_screenshot(
  app: &AppHandle,
  image: CapturedImage,
  suggested_file_stem: String,
) -> Result<(), String> {
  let preview = preview_png(&image);

  present(
    app,
    ExportArtifact::Screenshot {
      id: next_id(app),
      image,
      suggested_file_stem,
    },
    preview,
  )
}

/// Hands a finished recording to the export window.
///
/// Mirrors `present_screenshot`, with the poster standing in for the preview.
/// There is no full-resolution counterpart: the artifact is a movie, and the
/// only still it has is the one drawn when it finished.
pub fn present_recording(
  app: &AppHandle,
  info: FinalizeInfo,
  suggested_file_stem: String,
) -> Result<(), String> {
  let FinalizeInfo {
    camera,
    has_microphone,
    has_system_audio,
    duration_ms,
    height,
    path,
    poster,
    primary_kind,
    source_scale_factor,
    width,
  } = info;

  let mut audio_tracks = recording_audio_tracks(has_system_audio, has_microphone);
  if audio_tracks.is_empty() {
    audio_tracks = media_preview::inspect_audio_tracks(&path).unwrap_or_default();
  }

  present(
    app,
    ExportArtifact::Recording {
      id: next_id(app),
      audio_tracks,
      camera: camera.map(|camera| RecordingCamera {
        duration_ms: camera.duration_ms,
        height: camera.height,
        original_size_bytes: std::fs::metadata(&camera.path).map_or(0, |metadata| metadata.len()),
        path: camera.path,
        width: camera.width,
      }),
      duration_ms,
      height,
      path,
      primary_kind,
      source_scale_percent: scale_percent(source_scale_factor),
      suggested_file_stem,
      width,
    },
    poster,
  )
}

pub(super) fn take_artifact(app: &AppHandle) -> Option<ExportArtifact> {
  clear_recording_preview(app);
  let state = app.state::<ExportState>();
  let _ = state
    .preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();
  let _ = state
    .full_preview
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .take();

  artifact
}

/// Drops the pending artifact and puts the window away. Cancelling and closing
/// the window are the same act.
pub fn discard(app: &AppHandle) {
  if let Some(artifact) = take_artifact(app) {
    delete_working_file(&artifact);
  }
  let _ = window::hide(app);
  emit_snapshot(app);
}
