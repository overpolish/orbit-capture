// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::{Arc, Mutex};

use super::super::preview_platform::{workspace_editor::WorkspaceScene, RecordingPreviewSurface};
use super::super::ScreenshotWorkspaceOutputSettings;
use super::gesture::SelectionGestureOverride;
use crate::screenshots::CapturedImage;

#[derive(Default)]
pub(super) struct PreviewManager {
  pub(super) has_layout: bool,
  pub(super) latest_session_id: u64,
  pub(super) output: Option<ScreenshotWorkspaceOutputSettings>,
  pub(super) pane_target_size: Option<(u32, u32)>,
  pub(super) react_output: Option<ScreenshotWorkspaceOutputSettings>,
  pub(super) session_id: Option<u64>,
  pub(super) sources: Vec<(u64, Arc<CapturedImage>)>,
  pub(super) surface: Option<Arc<RecordingPreviewSurface>>,
  pub(super) selection_gesture: Option<SelectionGestureOverride>,
  pub(super) workspace_scene: Option<WorkspaceScene>,
}

impl PreviewManager {
  pub(super) fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That screenshot preview session is no longer active".to_owned())
  }

  pub(super) fn stop(&mut self) {
    if let Some(surface) = self.surface.as_ref() {
      surface.hide();
    }
    self.has_layout = false;
    self.output = None;
    self.pane_target_size = None;
    self.react_output = None;
    self.session_id = None;
    self.sources.clear();
    self.surface = None;
    self.selection_gesture = None;
    self.workspace_scene = None;
  }
}

#[derive(Default)]
pub struct ScreenshotPreviewState(pub(super) Mutex<PreviewManager>);
