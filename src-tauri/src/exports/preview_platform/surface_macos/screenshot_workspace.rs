// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native screenshot workspace presentation.
//!
//! Future screenshot annotation primitives enter through this retained GPU
//! workspace; React remains semantic state and command/event transport.

use super::ffi::screenwide_preview_surface_present_screenshot_workspace;
use super::native_types::{NativeWorkspaceLayer, NativeWorkspacePlacement};
use super::RecordingPreviewSurface;
use crate::screenshots::{native_canvas, CapturedImage, ScreenshotOutputSettings, StillOverlay};

impl RecordingPreviewSurface {
  /// Composes every screenshot item into one workspace drawable and command
  /// buffer. The native presenter retains the immutable source buffers, so a
  /// later pan/zoom redraw never requires React or another source upload.
  pub(crate) fn present_screenshot_workspace(
    &self,
    layers: &[(u64, &CapturedImage, ScreenshotOutputSettings)],
  ) -> Result<bool, String> {
    let mut native_layers = Vec::with_capacity(layers.len());
    for (index, (source_token, source, settings)) in layers.iter().enumerate() {
      let mut canvas = native_canvas(source.width, source.height, settings, true)?;
      canvas.foreground_only = u32::from(index > 0);
      native_layers.push(NativeWorkspaceLayer {
        pane_index: 0,
        // Screenshot selection and gesture events address layers by their
        // workspace order. `source_token` remains the independent cache key;
        // using it as the layer identity prevents the crop magnifier from
        // resolving the selected retained source.
        layer_id: u32::try_from(index).unwrap_or(u32::MAX - 1),
        source_rgba: source.rgba.as_ptr(),
        source_pixels: std::ptr::null_mut(),
        source_kind: 0,
        source_token: *source_token,
        source_width: source.width,
        source_height: source.height,
        canvas_width: settings.width,
        canvas_height: settings.height,
        canvas,
        placement: NativeWorkspacePlacement::default(),
        seconds: 0.0,
        cursor_rgba: std::ptr::null(),
        camera_rgba: std::ptr::null(),
        camera_pixels: std::ptr::null_mut(),
        overlay: StillOverlay::default(),
      });
    }
    Ok(unsafe {
      screenwide_preview_surface_present_screenshot_workspace(
        self.handle,
        native_layers.as_ptr(),
        native_layers.len().try_into().unwrap_or(u32::MAX),
      ) != 0
    })
  }
}
