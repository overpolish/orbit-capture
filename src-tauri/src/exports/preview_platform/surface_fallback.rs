// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Preview surface stub for platforms without a native compositing backend.
//!
//! Every entry point succeeds and does nothing, and `CAPABILITIES` reports
//! all-false so the frontend keeps the DOM/canvas preview. Replacing this file
//! is how a platform is ported: see the parent module for the contract, and
//! keep these exact names and signatures.

use tauri::WebviewWindow;

use super::{PreviewCapabilities, PreviewSurfaceRect};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

pub(super) const CAPABILITIES: PreviewCapabilities = PreviewCapabilities {
  native_recording_preview: false,
  native_screenshot_preview: false,
};

/// Stand-in for the macOS still-overlay uniform block, so shared callers can
/// name the parameter type before a backend defines a real one.
pub(crate) struct StillOverlay;

pub(crate) struct RecordingPreviewSurface;

unsafe impl Send for RecordingPreviewSurface {}
unsafe impl Sync for RecordingPreviewSurface {}

/// Nothing calls the present entry points while the backend is a stub, but
/// they are part of the contract a real backend has to fill in.
#[allow(dead_code)]
impl RecordingPreviewSurface {
  pub(crate) fn from_window(_window: &WebviewWindow) -> Result<Self, String> {
    Ok(Self)
  }

  pub(crate) fn set_viewport(&self, _rect: PreviewSurfaceRect, _backdrop: [f64; 4]) {}

  pub(crate) fn begin_layout(&self) {}

  pub(crate) fn set_scale(&self, _scale: f64) {}

  pub(crate) fn layout(&self, _index: u32, _rect: PreviewSurfaceRect, _defer_resize: bool) {}

  pub(crate) fn present(&self, _index: u32, _image: &CapturedImage) -> bool {
    false
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_composed(
    &self,
    _index: u32,
    _source_token: u64,
    _source: &CapturedImage,
    _settings: &ScreenshotOutputSettings,
    _seconds: f64,
    _cursor: Option<&CapturedImage>,
    _camera: Option<&CapturedImage>,
    _overlay: Option<&StillOverlay>,
    _clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    Ok(false)
  }

  pub(crate) fn present_screenshot_layer(
    &self,
    _index: u32,
    _source_token: u64,
    _source: &CapturedImage,
    _settings: &ScreenshotOutputSettings,
    _foreground_only: bool,
  ) -> Result<bool, String> {
    Ok(false)
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_composed_pixels(
    &self,
    _index: u32,
    _source_token: u64,
    _source_pixels: *mut std::ffi::c_void,
    _source_size: (u32, u32),
    _settings: &ScreenshotOutputSettings,
    _seconds: f64,
    _cursor: Option<&CapturedImage>,
    _camera: Option<&CapturedImage>,
    _camera_pixels: Option<*mut std::ffi::c_void>,
    _overlay: Option<&StillOverlay>,
    _clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    Ok(false)
  }

  pub(crate) fn present_batch(&self) -> PresentBatch<'_> {
    PresentBatch { _surface: self }
  }

  pub(crate) fn finish_layout(&self) {}

  pub(crate) fn hide(&self) {}
}

/// Presents are already atomic per pane here; the guard only mirrors macOS.
pub(crate) struct PresentBatch<'a> {
  _surface: &'a RecordingPreviewSurface,
}
