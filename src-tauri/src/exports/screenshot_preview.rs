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
use super::{ExportArtifact, ExportState};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

/// Decode width used before the webview has reported the on-screen pane size.
const FALLBACK_TARGET_WIDTH: u32 = 1_600;

#[derive(Default)]
struct PreviewManager {
  latest_session_id: u64,
  output: Option<ScreenshotOutputSettings>,
  pane_target_size: Option<(u32, u32)>,
  session_id: Option<u64>,
  source: Option<Arc<CapturedImage>>,
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
    self.output = None;
    self.pane_target_size = None;
    self.session_id = None;
    self.source = None;
    self.surface = None;
  }

  /// Mirrors the recording preview: composition happens at the on-screen pane
  /// size (never above the output size, never below the validation floor).
  fn scaled_output(&self, settings: &ScreenshotOutputSettings) -> ScreenshotOutputSettings {
    let target_width = self
      .pane_target_size
      .map(|size| size.0)
      .filter(|width| *width >= 16)
      .unwrap_or(FALLBACK_TARGET_WIDTH);
    let factor = (f64::from(target_width) / f64::from(settings.width.max(1))).min(1.0);
    if factor >= 1.0 {
      return settings.clone();
    }
    let minimum = (64.0 / f64::from(settings.width.max(1)))
      .max(64.0 / f64::from(settings.height.max(1)))
      .min(1.0);
    let factor = factor.max(minimum);
    let mut scaled = settings.clone();
    scaled.width = ((f64::from(settings.width) * factor).round().max(64.0)) as u32;
    scaled.height = ((f64::from(settings.height) * factor).round().max(64.0)) as u32;
    scaled
  }

  fn present(&self) -> Result<(), String> {
    let (Some(surface), Some(source), Some(output)) = (
      self.surface.as_ref(),
      self.source.as_ref(),
      self.output.as_ref(),
    ) else {
      return Ok(());
    };
    if output.width < 64 || output.height < 64 {
      return Ok(());
    }
    let scaled = self.scaled_output(output);
    surface
      .present_composed(0, 1, source, &scaled, 0.0, None, None, None, false)
      .map(|_| ())
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
  let source = {
    let export_state = app.state::<ExportState>();
    let artifact = export_state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(ExportArtifact::Screenshot { id, image, .. }) = artifact.as_ref() else {
      return Err("There is no screenshot to preview".to_owned());
    };
    if *id != artifact_id {
      return Err("That screenshot is no longer waiting to be exported".to_owned());
    }
    Arc::new(image.clone())
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
  manager.source = Some(source);
  manager.surface = surface;
  Ok(())
}

#[tauri::command]
pub fn layout_screenshot_preview_surface(
  state: tauri::State<'_, ScreenshotPreviewState>,
  backdrop: Option<[f64; 3]>,
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
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10]));
  for pane in panes {
    surface.layout(pane.index, pane.rect);
  }
  surface.finish_layout();
  if size_changed {
    manager.present()?;
  }
  Ok(())
}

#[tauri::command]
pub fn set_screenshot_preview_output(
  state: tauri::State<'_, ScreenshotPreviewState>,
  output: ScreenshotOutputSettings,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  manager.output = Some(output);
  manager.present()
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
