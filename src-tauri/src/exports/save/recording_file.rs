// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(in crate::exports) struct PrimaryRecordingSaveRequest<'a> {
  pub app: &'a AppHandle,
  pub artifact_id: u64,
  pub audio_tracks: &'a [RecordingAudioTrack],
  pub cancelled: &'a AtomicBool,
  pub compression: u8,
  pub cursor: Option<&'a Path>,
  pub cursor_effects: cursor_effects::CursorEffectSettings,
  pub directory: &'a Path,
  pub duration_ms: u64,
  pub height: u32,
  pub layout: track_selection::AudioLayout,
  pub progress_share: f64,
  pub resolution_scale_percent: u16,
  pub screen: &'a Path,
  pub selection: &'a track_selection::TrackSelection,
  pub source_scale_percent: u16,
  pub stem: &'a str,
  pub width: u32,
}

pub(in crate::exports) fn save_primary_recording(
  request: PrimaryRecordingSaveRequest<'_>,
) -> Result<Option<PathBuf>, String> {
  let video = media_preview::VideoExportOptions {
    compression: request.compression,
    resolution_scale_percent: request.resolution_scale_percent,
    source_scale_percent: request.source_scale_percent,
  };
  if let Some(cursor) = request.cursor {
    return cursor::save_baked(cursor::CursorSaveRequest {
      app: request.app,
      artifact_id: request.artifact_id,
      cancelled: request.cancelled,
      cursor,
      directory: request.directory,
      duration_ms: request.duration_ms,
      effects: request.cursor_effects,
      height: request.height,
      layout: request.layout,
      progress_share: request.progress_share,
      screen: request.screen,
      selection: request.selection,
      stem: request.stem,
      video,
      width: request.width,
    });
  }
  if request.compression == 0
    && request.resolution_scale_percent >= request.source_scale_percent
    && !request
      .selection
      .needs_processing(request.audio_tracks, request.layout)
  {
    return save_recording_copy(
      request.screen,
      request.directory,
      request.stem,
      media_preview::remuxer(),
    )
    .map(Some);
  }
  let mut on_progress = |processed_ms| {
    camera_save::emit_progress(
      request.app,
      request.artifact_id,
      "recording",
      processed_ms,
      request.duration_ms,
      0.0,
      request.progress_share,
    );
  };
  save_selected_recording_copy(
    request.screen,
    request.directory,
    request.stem,
    request.selection,
    request.layout,
    media_preview::ExportRunOptions {
      cancelled: request.cancelled,
      on_progress: &mut on_progress,
      video,
    },
    media_preview::selected_recording_exporter(),
  )
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
pub(in crate::exports) fn delivered_extension(working: &Path, can_remux: bool) -> &str {
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
pub(in crate::exports) fn save_recording(
  working: &Path,
  directory: &Path,
  stem: &str,
  remux: Option<media_preview::Remux>,
) -> Result<PathBuf, String> {
  let path = save_recording_copy(working, directory, stem, remux)?;
  let _ = std::fs::remove_file(working);
  Ok(path)
}

pub(in crate::exports) fn save_recording_copy(
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

#[cfg(test)]
pub(in crate::exports) fn save_selected_recording(
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
pub(in crate::exports) fn save_selected_recording_copy(
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

pub(in crate::exports) fn scale_percent(scale_factor: f32) -> u16 {
  (scale_factor.max(1.0) * 100.0).round().clamp(100.0, 400.0) as u16
}
