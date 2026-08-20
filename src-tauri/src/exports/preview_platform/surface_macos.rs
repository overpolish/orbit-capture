// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS preview surface: `CAMetalLayer` panes below the `WKWebView`.
//!
//! See the parent module for the contract a new platform has to satisfy. The
//! pane hierarchy, layout batching and GPU composition live in
//! `exports/recording_preview_surface_macos.m`; this file is only the FFI
//! boundary around it.

#[path = "surface_macos/callbacks.rs"]
mod callbacks;
#[path = "surface_macos/editor.rs"]
mod editor;
#[path = "surface_macos/ffi.rs"]
mod ffi;
#[path = "surface_macos/layout.rs"]
mod layout;
#[path = "surface_macos/native_types.rs"]
mod native_types;
#[path = "surface_macos/recording_workspace.rs"]
mod recording_workspace;
#[path = "surface_macos/screenshot_workspace.rs"]
mod screenshot_workspace;

use tauri::WebviewWindow;

use self::callbacks::release_callback_on_main;
pub(crate) use self::callbacks::run_on_main_queue;
use self::ffi::{
  screenwide_preview_surface_begin_present, screenwide_preview_surface_create,
  screenwide_preview_surface_destroy, screenwide_preview_surface_enable_editor,
  screenwide_preview_surface_end_present, screenwide_preview_surface_present,
  screenwide_preview_surface_set_selection_callback,
  screenwide_preview_surface_set_selection_gesture_callback,
};
pub(crate) use self::native_types::{NativeWorkspacePlacement, RecordingWorkspaceLayer};
use super::{SelectionCallback, SelectionGestureCallback, TransformCallback};
use crate::screenshots::CapturedImage;

pub(crate) struct RecordingPreviewSurface {
  pub(super) handle: *mut std::ffi::c_void,
  pub(super) selection_callback: Option<Box<SelectionCallback>>,
  pub(super) transform_callback: Option<Box<TransformCallback>>,
  pub(super) selection_gesture_callback: Option<Box<SelectionGestureCallback>>,
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
      Ok(Self {
        handle,
        selection_callback: None,
        transform_callback: None,
        selection_gesture_callback: None,
      })
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
}

impl Drop for RecordingPreviewSurface {
  fn drop(&mut self) {
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, None, std::ptr::null_mut());
      screenwide_preview_surface_set_selection_callback(self.handle, None, std::ptr::null_mut());
      screenwide_preview_surface_set_selection_gesture_callback(
        self.handle,
        None,
        std::ptr::null_mut(),
      );
      screenwide_preview_surface_destroy(self.handle);
    }
    release_callback_on_main(self.transform_callback.take());
    release_callback_on_main(self.selection_callback.take());
    release_callback_on_main(self.selection_gesture_callback.take());
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

#[cfg(test)]
mod tests {
  unsafe extern "C" {
    fn screenwide_gpu_still_presenter_create() -> *mut std::ffi::c_void;
    fn screenwide_gpu_still_presenter_destroy(handle: *mut std::ffi::c_void);
  }

  #[test]
  fn retained_workspace_metal_shader_compiles() {
    let presenter = unsafe { screenwide_gpu_still_presenter_create() };
    assert!(!presenter.is_null());
    unsafe { screenwide_gpu_still_presenter_destroy(presenter) };
  }
}
