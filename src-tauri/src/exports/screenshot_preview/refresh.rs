// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::super::{ExportArtifact, ExportState};
use super::state::{PreviewManager, ScreenshotPreviewState};

/// Refreshes captured sources without replacing the live Metal surface.
/// Screenshot workspaces append items while the export window stays open;
/// restarting the surface at that point can overlap an in-flight layout and
/// drawable presentation from the previous session.
#[tauri::command]
pub async fn refresh_screenshot_preview_sources(
  app: AppHandle,
  state: tauri::State<'_, ScreenshotPreviewState>,
  artifact_id: u64,
  session_id: u64,
) -> Result<(), String> {
  let existing = {
    let manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.sources.clone()
  };
  let sources = {
    let export_state = app.state::<ExportState>();
    let artifact = export_state
      .screenshot
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(ExportArtifact::Screenshot { id, items, .. }) = artifact.as_ref() else {
      return Err("There is no screenshot to preview".to_owned());
    };
    if *id != artifact_id {
      return Err("That screenshot is no longer waiting to be exported".to_owned());
    }
    items
      .iter()
      .map(|item| {
        existing
          .iter()
          .find(|(id, _)| *id == item.id)
          .map(|(_, image)| (item.id, Arc::clone(image)))
          .unwrap_or_else(|| (item.id, Arc::new(item.image.clone())))
      })
      .collect::<Vec<_>>()
  };
  let presentation = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.sources = sources;
    manager.has_layout.then(|| {
      (
        manager.surface.clone(),
        manager.output.clone(),
        manager.sources.clone(),
      )
    })
  };
  if let Some((Some(surface), Some(output), sources)) = presentation {
    let batch = surface.present_batch();
    let staged = PreviewManager::present_snapshot(&surface, &output, &sources)?;
    drop(batch);
    if !staged {
      PreviewManager::present_once_pane_exists(&app, session_id, 0);
    }
  }
  Ok(())
}
