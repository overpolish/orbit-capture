// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native screenshot editing preview.
//!
//! The screenshot is a single static image, so the whole editing loop is one
//! GPU composition into the same native pane surface the recording preview
//! uses: the source uploads once (the presenter caches it by token), and each
//! settings change is a uniform-only compute pass. No pixels ever cross IPC.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager};

use super::preview_platform::{PreviewSurfaceRect, RecordingPreviewSurface};
use super::{ExportArtifact, ExportState, ScreenshotWorkspaceOutputSettings};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

/// Decode width used before the webview has reported the on-screen pane size.
const FALLBACK_TARGET_WIDTH: u32 = 1_600;

#[derive(Default)]
struct PreviewManager {
  has_layout: bool,
  latest_session_id: u64,
  output: Option<ScreenshotWorkspaceOutputSettings>,
  pane_target_size: Option<(u32, u32)>,
  session_id: Option<u64>,
  sources: Vec<(u64, Arc<CapturedImage>)>,
  surface: Option<Arc<RecordingPreviewSurface>>,
}

impl PreviewManager {
  fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That screenshot preview session is no longer active".to_owned())
  }

  fn stop(&mut self) {
    if let Some(surface) = self.surface.as_ref() {
      surface.hide();
    }
    self.has_layout = false;
    self.output = None;
    self.pane_target_size = None;
    self.session_id = None;
    self.sources.clear();
    self.surface = None;
  }

  /// Mirrors the recording preview: composition happens at the on-screen pane
  /// size (never above the output size, never below the validation floor).
  fn scaled_output(&self, settings: &ScreenshotOutputSettings) -> ScreenshotOutputSettings {
    let (target_width, target_height) = self
      .pane_target_size
      .filter(|size| size.0 >= 16 && size.1 >= 16)
      .unwrap_or((
        FALLBACK_TARGET_WIDTH,
        ((f64::from(FALLBACK_TARGET_WIDTH) * f64::from(settings.height)
          / f64::from(settings.width.max(1)))
        .round()
        .max(1.0)) as u32,
      ));
    let factor = (f64::from(target_width) / f64::from(settings.width.max(1))).min(1.0);
    let height_factor = (f64::from(target_height) / f64::from(settings.height.max(1))).min(1.0);
    if factor >= 1.0 && height_factor >= 1.0 {
      return settings.clone();
    }
    let minimum = (64.0 / f64::from(settings.width.max(1)))
      .max(64.0 / f64::from(settings.height.max(1)))
      .min(1.0);
    let factor = factor.max(minimum);
    let height_factor = height_factor.max(minimum);
    let mut scaled = settings.clone();
    scaled.width = ((f64::from(settings.width) * factor).round().max(64.0)) as u32;
    scaled.height = ((f64::from(settings.height) * height_factor)
      .round()
      .max(64.0)) as u32;
    scaled
  }

  fn present(&self) -> Result<(), String> {
    let (Some(surface), Some(output)) = (self.surface.as_ref(), self.output.as_ref()) else {
      return Ok(());
    };
    if output.canvas.width < 64 || output.canvas.height < 64 {
      return Ok(());
    }
    for (index, item_output) in output.items.iter().enumerate() {
      let Some((_, source)) = self.sources.iter().find(|(id, _)| *id == item_output.id) else {
        continue;
      };
      let scaled = self.scaled_output(&output.output_for_id(item_output.id));
      surface.present_screenshot_layer(index as u32, item_output.id, source, &scaled, index > 0)?;
    }
    Ok(())
  }
}

#[derive(Default)]
pub struct ScreenshotPreviewState(Mutex<PreviewManager>);

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSurfacePane {
  index: u32,
  rect: PreviewSurfaceRect,
}

#[tauri::command]
pub fn start_screenshot_preview(
  app: AppHandle,
  state: tauri::State<'_, ScreenshotPreviewState>,
  artifact_id: u64,
  session_id: u64,
) -> Result<(), String> {
  let sources = {
    let export_state = app.state::<ExportState>();
    let artifact = export_state
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
      .map(|item| (item.id, Arc::new(item.image.clone())))
      .collect::<Vec<_>>()
  };
  let surface = app
    .get_webview_window("export")
    .map(|window| RecordingPreviewSurface::from_window(&window).map(Arc::new))
    .transpose()?;
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  if session_id < manager.latest_session_id {
    return Ok(());
  }
  manager.stop();
  manager.latest_session_id = session_id;
  manager.session_id = Some(session_id);
  manager.sources = sources;
  manager.surface = surface;
  Ok(())
}

// Async so Tauri dispatches it off the main thread: this command blocks on a
// DirectComposition commit, and the main thread pumps the Win32 messages that
// deliver the webview's pointer input.
#[tauri::command]
pub async fn layout_screenshot_preview_surface(
  state: tauri::State<'_, ScreenshotPreviewState>,
  backdrop: Option<[f64; 4]>,
  output: ScreenshotWorkspaceOutputSettings,
  panes: Vec<ScreenshotSurfacePane>,
  scale: f64,
  session_id: u64,
  viewport: PreviewSurfaceRect,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  let output_changed = manager.output.as_ref() != Some(&output);
  manager.output = Some(output);
  let scale = if scale.is_finite() && scale > 0.0 {
    scale
  } else {
    1.0
  };
  let mut size_changed = false;
  if let Some(pane) = panes.first() {
    let next = (
      (pane.rect.width * scale).round().max(2.0) as u32,
      (pane.rect.height * scale).round().max(2.0) as u32,
    );
    if manager.pane_target_size != Some(next) {
      manager.pane_target_size = Some(next);
      size_changed = true;
    }
  }
  let Some(surface) = manager.surface.as_ref() else {
    return Ok(());
  };
  // Lay out first with the pane frames held back, then present: the batch
  // applies the deferred frames in the same Core Animation transaction as the
  // freshly composed drawables. Presenting before layout does not achieve
  // that - an explicit transaction opened outside any implicit one commits
  // immediately, so the drawable would land a tick before the frame and be
  // fitted into the old rect meanwhile. A layout that will not present (a
  // pure pan) applies its frames at once, or they would never land.
  let will_present = !manager.has_layout || output_changed || size_changed;
  surface.set_scale(scale);
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10, 1.0]));
  for pane in panes {
    surface.layout(pane.index, pane.rect, will_present);
  }
  // Open the batch before `finish_layout` so the hides, the deferred pane
  // frames and the fresh layer presents all land in one commit - on Windows
  // that is also the invoke's single compositor wait.
  let batch = will_present.then(|| surface.present_batch());
  surface.finish_layout();
  if will_present {
    manager.present()?;
  }
  drop(batch);
  manager.has_layout = true;
  Ok(())
}

#[tauri::command]
pub fn stop_screenshot_preview(
  state: tauri::State<'_, ScreenshotPreviewState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  if manager.session_id == Some(session_id) {
    manager.stop();
  }
  Ok(())
}
