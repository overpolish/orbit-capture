// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) struct AudioSaveRequest<'a> {
  pub(super) app: &'a AppHandle,
  pub(super) cancelled: &'a AtomicBool,
  pub(super) directory: &'a Path,
  pub(super) duration_ms: u64,
  pub(super) id: u64,
  pub(super) layout: track_selection::AudioLayout,
  pub(super) selected_any: bool,
  pub(super) selection: &'a track_selection::TrackSelection,
  pub(super) stem: &'a str,
  pub(super) working: &'a Path,
}

pub(super) fn save_audio(request: AudioSaveRequest<'_>) -> Result<Option<PathBuf>, String> {
  let AudioSaveRequest {
    app,
    cancelled,
    directory,
    duration_ms,
    id,
    layout,
    selected_any,
    selection,
    stem,
    working,
  } = request;
  if !selected_any {
    return Err("Select at least one audio track to export".to_owned());
  }
  let path = unique_path(directory, stem, AUDIO_EXTENSION, &|candidate| {
    candidate.exists()
  });
  let exporter = media_preview::selected_recording_exporter()
    .ok_or_else(|| "FFmpeg is required to save an audio recording".to_owned())?;
  let mut on_progress = |processed_ms| {
    camera_save::emit_progress(app, id, "recording", processed_ms, duration_ms, 0.0, 99.0);
  };
  let run = media_preview::ExportRunOptions {
    cancelled,
    on_progress: &mut on_progress,
    video: media_preview::VideoExportOptions {
      compression: 0,
      resolution_scale_percent: 100,
      source_scale_percent: 100,
    },
  };
  match exporter(working, &path, selection, layout, run)? {
    media_preview::ExportRunResult::Completed if path.is_file() => {
      let _ = std::fs::remove_file(working);
      Ok(Some(path))
    }
    media_preview::ExportRunResult::Completed => {
      Err("The exported audio did not finish publishing".to_owned())
    }
    media_preview::ExportRunResult::Cancelled => Ok(None),
  }
}
