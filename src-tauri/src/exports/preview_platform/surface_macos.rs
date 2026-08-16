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
  fn screenwide_preview_surface_create(host_view: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
  fn screenwide_preview_surface_layout(
    handle: *mut std::ffi::c_void,
    index: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    defer_resize: i32,
  );
  fn screenwide_preview_surface_set_viewport(
    handle: *mut std::ffi::c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
  );
  fn screenwide_preview_surface_begin_layout(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_finish_layout(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_begin_present(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_end_present(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_present(
    handle: *mut std::ffi::c_void,
    index: u32,
    rgba: *const u8,
    width: u32,
    height: u32,
  ) -> i32;
  fn screenwide_preview_surface_present_composed(
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
  fn screenwide_preview_surface_present_composed_pixels(
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
  fn screenwide_preview_surface_hide(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_destroy(handle: *mut std::ffi::c_void);
}

pub(crate) struct RecordingPreviewSurface {
  handle: *mut std::ffi::c_void,
}

unsafe impl Send for RecordingPreviewSurface {}
unsafe impl Sync for RecordingPreviewSurface {}

impl RecordingPreviewSurface {
  pub(crate) fn from_window(window: &WebviewWindow) -> Result<Self, String> {
    let host_view = window.ns_view().map_err(|error| error.to_string())?;
    let handle = unsafe { screenwide_preview_surface_create(host_view) };
    if handle.is_null() {
      Err("The native recording preview surface could not be created".to_owned())
    } else {
      Ok(Self { handle })
    }
  }

  pub(crate) fn set_viewport(&self, rect: PreviewSurfaceRect, backdrop: [f64; 4]) {
    unsafe {
      screenwide_preview_surface_set_viewport(
        self.handle,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        backdrop[0],
        backdrop[1],
        backdrop[2],
        // The export window is transparent, so re-blending the sampled CSS
        // stack against AppKit's backing material shifts #1c1c1c to #1d1d1d.
        // Its RGB is already the final composited WebView colour.
        1.0,
      );
    }
  }

  pub(crate) fn begin_layout(&self) {
    unsafe {
      screenwide_preview_surface_begin_layout(self.handle);
    }
  }

  pub(crate) fn set_scale(&self, _scale: f64) {}

  /// `defer_resize` holds back a size change until the next present so the
  /// new pane rect and its re-composed frame reach the screen together; pass
  /// it when a present for this layout is on its way.
  pub(crate) fn layout(&self, index: u32, rect: PreviewSurfaceRect, defer_resize: bool) {
    unsafe {
      screenwide_preview_surface_layout(
        self.handle,
        index,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        i32::from(defer_resize),
      );
    }
  }

  pub(crate) fn present(&self, index: u32, image: &CapturedImage) -> bool {
    unsafe {
      screenwide_preview_surface_present(
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
      screenwide_preview_surface_present_composed(
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

  pub(crate) fn present_screenshot_layer(
    &self,
    index: u32,
    source_token: u64,
    source: &CapturedImage,
    settings: &ScreenshotOutputSettings,
    foreground_only: bool,
  ) -> Result<bool, String> {
    let mut canvas = native_canvas(source.width, source.height, settings, true)?;
    canvas.foreground_only = u32::from(foreground_only);
    Ok(unsafe {
      screenwide_preview_surface_present_composed(
        self.handle,
        index,
        source_token,
        source.rgba.as_ptr(),
        source.width,
        source.height,
        settings.width,
        settings.height,
        std::ptr::from_ref(&canvas),
        0.0,
        std::ptr::null(),
        std::ptr::null(),
        std::ptr::null(),
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
      screenwide_preview_surface_present_composed_pixels(
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

  /// Opens a present batch: every present until the guard drops lands in one
  /// Core Animation commit together with all frames deferred by `layout`.
  /// Dropping the guard flushes even when nothing was presented, so a
  /// deferred layout never strands the panes.
  pub(crate) fn present_batch(&self) -> PresentBatch<'_> {
    unsafe {
      screenwide_preview_surface_begin_present(self.handle);
    }
    PresentBatch { surface: self }
  }

  pub(crate) fn finish_layout(&self) {
    unsafe {
      screenwide_preview_surface_finish_layout(self.handle);
    }
  }

  pub(crate) fn hide(&self) {
    unsafe {
      screenwide_preview_surface_hide(self.handle);
    }
  }
}

impl Drop for RecordingPreviewSurface {
  fn drop(&mut self) {
    unsafe {
      screenwide_preview_surface_destroy(self.handle);
    }
  }
}

pub(crate) struct PresentBatch<'a> {
  surface: &'a RecordingPreviewSurface,
}

impl Drop for PresentBatch<'_> {
  fn drop(&mut self) {
    unsafe {
      screenwide_preview_surface_end_present(self.surface.handle);
    }
  }
}
