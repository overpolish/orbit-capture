// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::preview_platform::PreviewSurfaceRect;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSurfacePane {
  #[allow(dead_code)]
  pub(super) index: u32,
  pub(super) rect: PreviewSurfaceRect,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSelectionOverlay {
  #[serde(default)]
  pub(super) crop_mode: bool,
  #[serde(default)]
  pub(super) image: Option<PreviewSurfaceRect>,
  pub(super) layer_id: Option<u32>,
  pub(super) pane_index: u32,
  pub(super) radius_percent: f64,
  pub(super) rect: PreviewSurfaceRect,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScreenshotPreviewTransformEvent {
  pub(super) session_id: u64,
  pub(super) zoom_percent: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScreenshotSelectionGestureEvent {
  pub(super) delta_x: f64,
  pub(super) delta_y: f64,
  pub(super) edges: u32,
  pub(super) operation: u32,
  pub(super) pane_index: u32,
  pub(super) phase: &'static str,
  pub(super) scale: f64,
  pub(super) session_id: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScreenshotSelectionChangeEvent {
  pub(super) pane_index: Option<u32>,
  pub(super) session_id: u64,
}
