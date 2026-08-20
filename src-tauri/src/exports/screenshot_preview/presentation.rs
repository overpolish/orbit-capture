// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::super::preview_platform::RecordingPreviewSurface;
use super::super::ScreenshotWorkspaceOutputSettings;
use super::state::{PreviewManager, ScreenshotPreviewState};
use crate::screenshots::CapturedImage;

impl PreviewManager {
  pub(super) fn present(&self) -> Result<(), String> {
    let (Some(surface), Some(output)) = (self.surface.as_ref(), self.output.as_ref()) else {
      return Ok(());
    };
    Self::present_snapshot(surface, output, &self.sources).map(|_| ())
  }

  /// Stages the current sources on the native workspace. `Ok(false)` means the
  /// pane was not there to stage them on yet (see `present_once_pane_exists`).
  #[allow(clippy::needless_return)]
  pub(super) fn present_snapshot(
    surface: &RecordingPreviewSurface,
    output: &ScreenshotWorkspaceOutputSettings,
    sources: &[(u64, Arc<CapturedImage>)],
  ) -> Result<bool, String> {
    if output.canvas.width < 64 || output.canvas.height < 64 {
      return Ok(true);
    }
    #[cfg(target_os = "macos")]
    {
      let layers = output
        .items
        .iter()
        .filter_map(|item_output| {
          let (_, source) = sources.iter().find(|(id, _)| *id == item_output.id)?;
          Some((
            item_output.id,
            source.as_ref(),
            output.output_for_id(item_output.id),
          ))
        })
        .collect::<Vec<_>>();
      if layers.is_empty() {
        return Ok(true);
      }
      let staged = surface.present_screenshot_workspace(&layers)?;
      return Ok(staged);
    }
    #[cfg(not(target_os = "macos"))]
    for (index, item_output) in output.items.iter().enumerate() {
      let Some((_, source)) = sources.iter().find(|(id, _)| *id == item_output.id) else {
        continue;
      };
      let item_settings = output.output_for_id(item_output.id);
      surface.present_screenshot_layer(
        index as u32,
        item_output.id,
        source,
        &item_settings,
        index > 0,
      )?;
    }
    #[cfg(not(target_os = "macos"))]
    Ok(true)
  }

  pub(super) fn present_batch(&self) -> Result<(), String> {
    let batch = self.surface.as_ref().map(|surface| surface.present_batch());
    let result = self.present();
    drop(batch);
    result
  }

  /// Presents from the main thread once the native pane exists.
  ///
  /// The async layout and source-refresh commands queue the pane's creation on
  /// the main thread but stage their present straight away from the worker, so
  /// the first present of a session (and a refresh racing it) can find no pane
  /// and be dropped - nothing draws until the user's next interaction. Each
  /// attempt here is a separate main-thread turn, so the queued layout blocks
  /// run in between; attempts stop once the present lands or the session ends.
  ///
  /// On macOS the attempt is queued on the dispatch main queue, behind the
  /// layout blocks themselves. Tauri's event-loop proxy is a separate queue
  /// with no ordering against them, and with a second export window's player
  /// keeping the main queue busy all thirty proxy turns can come and go before
  /// the pane's layout block ever runs - the screenshot then stays blank until
  /// the first gesture.
  pub(super) fn present_once_pane_exists(app: &AppHandle, session_id: u64, attempt: u32) {
    const ATTEMPT_LIMIT: u32 = 30;
    let handle = app.clone();
    let work = move || {
      let presentation = {
        let state = handle.state::<ScreenshotPreviewState>();
        let Ok(manager) = state.0.lock() else { return };
        if manager.require_session(session_id).is_err() {
          return;
        }
        (
          manager.surface.clone(),
          manager.output.clone(),
          manager.sources.clone(),
        )
      };
      let (Some(surface), Some(output), sources) = presentation else {
        return;
      };
      let staged = Self::present_snapshot(&surface, &output, &sources).unwrap_or(true);
      if !staged && attempt + 1 < ATTEMPT_LIMIT {
        Self::present_once_pane_exists(&handle, session_id, attempt + 1);
      } else if !staged {
        eprintln!(
          "Screenshot preview gave up waiting for its pane (session {session_id}); the next gesture will draw it"
        );
      }
    };
    #[cfg(target_os = "macos")]
    super::preview_platform::run_on_main_queue(Box::new(work));
    #[cfg(not(target_os = "macos"))]
    let _ = app.run_on_main_thread(work);
  }
}
