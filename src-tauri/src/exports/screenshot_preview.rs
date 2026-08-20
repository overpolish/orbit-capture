// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native screenshot editing preview.
//!
//! The screenshot is a single static image, so the whole editing loop is one
//! GPU composition into the same native pane surface the recording preview
//! uses: the source uploads once (the presenter caches it by token), and each
//! settings change is a uniform-only compute pass. No pixels ever cross IPC.

mod controls;
mod geometry;
mod gesture;
mod layout;
mod payloads;
mod presentation;
mod refresh;
mod start;
mod state;

pub use controls::{
  __cmd__center_screenshot_preview_workspace, __cmd__set_screenshot_preview_zoom,
  __cmd__stop_screenshot_preview, __tauri_command_name_center_screenshot_preview_workspace,
  __tauri_command_name_set_screenshot_preview_zoom, __tauri_command_name_stop_screenshot_preview,
  center_screenshot_preview_workspace, set_screenshot_preview_zoom, stop_screenshot_preview,
};
pub use layout::{
  __cmd__layout_screenshot_preview_surface, __tauri_command_name_layout_screenshot_preview_surface,
  layout_screenshot_preview_surface,
};
#[allow(unused_imports)]
pub use payloads::{ScreenshotSelectionOverlay, ScreenshotSurfacePane};
pub use refresh::{
  __cmd__refresh_screenshot_preview_sources,
  __tauri_command_name_refresh_screenshot_preview_sources, refresh_screenshot_preview_sources,
};
pub use start::{
  __cmd__start_screenshot_preview, __tauri_command_name_start_screenshot_preview,
  start_screenshot_preview,
};
pub use state::ScreenshotPreviewState;

// Native screenshot document extensions enter through presentation; React
// remains the semantic settings, history, and command/event transport layer.
pub(super) use super::{preview_platform, preview_workspace_model};
