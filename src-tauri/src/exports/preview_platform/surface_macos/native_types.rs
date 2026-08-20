// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::screenshots::{CapturedImage, NativeCanvas, ScreenshotOutputSettings, StillOverlay};

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct NativeWorkspacePlacement {
  pub(super) x: i32,
  pub(super) y: i32,
  pub(super) width: u32,
  pub(super) height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct NativeWorkspacePaneRect {
  pub(super) index: u32,
  pub(super) x: f64,
  pub(super) y: f64,
  pub(super) width: f64,
  pub(super) height: f64,
}

#[repr(C)]
pub(super) struct NativeWorkspaceLayer {
  pub(super) pane_index: u32,
  pub(super) layer_id: u32,
  pub(super) source_rgba: *const u8,
  pub(super) source_pixels: *mut std::ffi::c_void,
  pub(super) source_kind: u32,
  pub(super) source_token: u64,
  pub(super) source_width: u32,
  pub(super) source_height: u32,
  pub(super) canvas_width: u32,
  pub(super) canvas_height: u32,
  pub(super) canvas: NativeCanvas,
  pub(super) placement: NativeWorkspacePlacement,
  pub(super) seconds: f64,
  pub(super) cursor_rgba: *const u8,
  pub(super) camera_rgba: *const u8,
  pub(super) camera_pixels: *mut std::ffi::c_void,
  pub(super) overlay: StillOverlay,
}

/// Input for one layer in the retained recording workspace. A decoded RGBA
/// image or a native CVPixelBuffer may be supplied; optional cursor/camera
/// buffers and overlay uniforms are composed in the same Metal pass.
pub(crate) struct RecordingWorkspaceLayer<'a> {
  pub pane_index: u32,
  pub source_token: u64,
  pub source: Option<&'a CapturedImage>,
  pub source_pixels: Option<(*mut std::ffi::c_void, (u32, u32))>,
  pub settings: ScreenshotOutputSettings,
  pub placement: NativeWorkspacePlacement,
  pub seconds: f64,
  pub cursor: Option<&'a CapturedImage>,
  pub camera: Option<&'a CapturedImage>,
  pub camera_pixels: Option<(*mut std::ffi::c_void, (u32, u32))>,
  pub overlay: Option<&'a StillOverlay>,
  pub clip_cursor_at_video_edge: bool,
  pub foreground_only: bool,
}
