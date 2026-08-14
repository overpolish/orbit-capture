// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS preview surface: `CAMetalLayer` panes below the `WKWebView`.
//!
//! See the parent module for the contract a new platform has to satisfy. The
//! pane hierarchy, layout batching and GPU composition live in
//! `exports/recording_preview_surface_macos.m`; this file is only the FFI
//! boundary around it.

use tauri::WebviewWindow;

use super::{PreviewCapabilities, PreviewSurfaceRect};
use crate::screenshots::{
  native_canvas, CapturedImage, NativeCanvas, ScreenshotOutputSettings, StillOverlay,
};

pub(super) const CAPABILITIES: PreviewCapabilities = PreviewCapabilities {
  native_recording_preview: true,
  native_screenshot_preview: true,
};

unsafe extern "C" {
  fn orbit_preview_surface_create(host_view: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
  fn orbit_preview_surface_layout(
    handle: *mut std::ffi::c_void,
    index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
  );
  fn orbit_preview_surface_set_viewport(
    handle: *mut std::ffi::c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    red: f64,
    green: f64,
    blue: f64,
  );
  fn orbit_preview_surface_begin_layout(handle: *mut std::ffi::c_void);
  fn orbit_preview_surface_finish_layout(handle: *mut std::ffi::c_void);
  fn orbit_preview_surface_present(
    handle: *mut std::ffi::c_void,
    index: u32,
    rgba: *const u8,
    width: u32,
    height: u32,
  ) -> i32;
  fn orbit_preview_surface_present_composed(
    handle: *mut std::ffi::c_void,
    index: u32,
    source_token: u64,
    source_rgba: *const u8,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    canvas: *const NativeCanvas,
    seconds: f64,
    cursor_rgba: *const u8,
    camera_rgba: *const u8,
    overlay: *const StillOverlay,
  ) -> i32;
  fn orbit_preview_surface_present_composed_pixels(
    handle: *mut std::ffi::c_void,
    index: u32,
    source_token: u64,
    source_pixels: *mut std::ffi::c_void,
    output_width: u32,
    output_height: u32,
    canvas: *const NativeCanvas,
    seconds: f64,
    cursor_rgba: *const u8,
    camera_rgba: *const u8,
    camera_pixels: *mut std::ffi::c_void,
    overlay: *const StillOverlay,
  ) -> i32;
  fn orbit_preview_surface_hide(handle: *mut std::ffi::c_void);
  fn orbit_preview_surface_destroy(handle: *mut std::ffi::c_void);
}

pub(crate) struct RecordingPreviewSurface {
  handle: *mut std::ffi::c_void,
}

unsafe impl Send for RecordingPreviewSurface {}
unsafe impl Sync for RecordingPreviewSurface {}

impl RecordingPreviewSurface {
  pub(crate) fn from_window(window: &WebviewWindow) -> Result<Self, String> {
    let host_view = window.ns_view().map_err(|error| error.to_string())?;
    let handle = unsafe { orbit_preview_surface_create(host_view) };
    if handle.is_null() {
      Err("The native recording preview surface could not be created".to_owned())
    } else {
      Ok(Self { handle })
    }
  }

  pub(crate) fn set_viewport(&self, rect: PreviewSurfaceRect, backdrop: [f64; 4]) {
    // AppKit's container remains opaque. Flatten over black to preserve the
    // established macOS appearance while Windows retains the live alpha.
    let backdrop = [
      backdrop[0] * backdrop[3],
      backdrop[1] * backdrop[3],
      backdrop[2] * backdrop[3],
    ];
    unsafe {
      orbit_preview_surface_set_viewport(
        self.handle,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        backdrop[0],
        backdrop[1],
        backdrop[2],
      );
    }
  }

  pub(crate) fn begin_layout(&self) {
    unsafe {
      orbit_preview_surface_begin_layout(self.handle);
    }
  }

  pub(crate) fn set_scale(&self, _scale: f64) {}

  pub(crate) fn layout(&self, index: u32, rect: PreviewSurfaceRect) {
    unsafe {
      orbit_preview_surface_layout(self.handle, index, rect.x, rect.y, rect.width, rect.height);
    }
  }

  pub(crate) fn present(&self, index: u32, image: &CapturedImage) -> bool {
    unsafe {
      orbit_preview_surface_present(
        self.handle,
        index,
        image.rgba.as_ptr(),
        image.width,
        image.height,
      ) != 0
    }
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_composed(
    &self,
    index: u32,
    source_token: u64,
    source: &CapturedImage,
    settings: &ScreenshotOutputSettings,
    seconds: f64,
    cursor: Option<&CapturedImage>,
    camera: Option<&CapturedImage>,
    overlay: Option<&StillOverlay>,
    clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    let mut canvas = native_canvas(source.width, source.height, settings, true)?;
    canvas.clip_cursor_at_video_edge = u32::from(clip_cursor_at_video_edge);
    Ok(unsafe {
      orbit_preview_surface_present_composed(
        self.handle,
        index,
        source_token,
        source.rgba.as_ptr(),
        source.width,
        source.height,
        settings.width,
        settings.height,
        std::ptr::from_ref(&canvas),
        seconds,
        cursor.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
        camera.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
        overlay.map_or(std::ptr::null(), std::ptr::from_ref),
      ) != 0
    })
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_composed_pixels(
    &self,
    index: u32,
    source_token: u64,
    source_pixels: *mut std::ffi::c_void,
    source_size: (u32, u32),
    settings: &ScreenshotOutputSettings,
    seconds: f64,
    cursor: Option<&CapturedImage>,
    camera: Option<&CapturedImage>,
    camera_pixels: Option<*mut std::ffi::c_void>,
    overlay: Option<&StillOverlay>,
    clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    let mut canvas = native_canvas(source_size.0, source_size.1, settings, true)?;
    canvas.clip_cursor_at_video_edge = u32::from(clip_cursor_at_video_edge);
    Ok(unsafe {
      orbit_preview_surface_present_composed_pixels(
        self.handle,
        index,
        source_token,
        source_pixels,
        settings.width,
        settings.height,
        std::ptr::from_ref(&canvas),
        seconds,
        cursor.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
        camera.map_or(std::ptr::null(), |image| image.rgba.as_ptr()),
        camera_pixels.unwrap_or(std::ptr::null_mut()),
        overlay.map_or(std::ptr::null(), std::ptr::from_ref),
      ) != 0
    })
  }

  pub(crate) fn finish_layout(&self) {
    unsafe {
      orbit_preview_surface_finish_layout(self.handle);
    }
  }

  pub(crate) fn hide(&self) {
    unsafe {
      orbit_preview_surface_hide(self.handle);
    }
  }
}

impl Drop for RecordingPreviewSurface {
  fn drop(&mut self) {
    unsafe {
      orbit_preview_surface_destroy(self.handle);
    }
  }
}
