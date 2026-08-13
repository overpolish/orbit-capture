// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) struct CursorSaveRequest<'a> {
  pub app: &'a AppHandle,
  pub artifact_id: u64,
  pub cancelled: &'a AtomicBool,
  pub cursor: &'a Path,
  pub directory: &'a Path,
  pub duration_ms: u64,
  pub effects: cursor_effects::CursorEffectSettings,
  pub height: u32,
  pub layout: track_selection::AudioLayout,
  pub output: &'a ScreenshotOutputSettings,
  pub progress_share: f64,
  pub screen: &'a Path,
  pub selection: &'a track_selection::TrackSelection,
  pub stem: &'a str,
  pub video: media_preview::VideoExportOptions,
  pub width: u32,
}

pub(super) fn save_baked(request: CursorSaveRequest<'_>) -> Result<Option<PathBuf>, String> {
  let path = unique_path(
    request.directory,
    request.stem,
    RECORDING_EXTENSION,
    &|candidate| candidate.exists(),
  );
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
  match cursor_export::export(cursor_export::CursorExportRequest {
    audio_layout: request.layout,
    audio_source: None,
    camera: None,
    cancelled: request.cancelled,
    cursor: Some(request.cursor),
    cursor_effects: request.effects,
    destination: &path,
    duration_ms: request.duration_ms,
    height: request.height,
    on_progress: &mut on_progress,
    screen: request.screen,
    selection: request.selection,
    output: request.output,
    video: request.video,
    width: request.width,
  })? {
    media_preview::ExportRunResult::Completed => Ok(Some(path)),
    media_preview::ExportRunResult::Cancelled => Ok(None),
  }
}

pub(super) fn remove_working_file(cursor: Option<&RecordingCursor>) {
  if let Some(cursor) = cursor {
    let _ = std::fs::remove_file(&cursor.path);
  }
}
