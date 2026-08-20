// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::{
  PreviewSelection, SelectionCallback, SelectionGestureCallback, TransformCallback,
};
use super::callbacks::{
  release_callback_on_main, selection_callback, selection_gesture_callback, transform_callback,
};
use super::ffi::{
  screenwide_preview_surface_center_editor, screenwide_preview_surface_enable_editor,
  screenwide_preview_surface_set_editor_zoom, screenwide_preview_surface_set_selection,
  screenwide_preview_surface_set_selection_callback,
  screenwide_preview_surface_set_selection_gesture_callback,
  screenwide_preview_surface_set_selection_snapping,
  screenwide_preview_surface_set_selection_targets,
  screenwide_preview_surface_set_selection_visible,
};
use super::RecordingPreviewSurface;

impl RecordingPreviewSurface {
  pub(crate) fn enable_editor(&mut self, callback: TransformCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut TransformCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, Some(transform_callback), context);
    }
    release_callback_on_main(self.transform_callback.replace(callback));
  }

  pub(crate) fn set_editor_active(&self, active: bool) {
    let (callback, context) = if active {
      self
        .transform_callback
        .as_ref()
        .map_or((None, std::ptr::null_mut()), |callback| {
          (
            Some(transform_callback as unsafe extern "C" fn(f64, *mut std::ffi::c_void)),
            (&**callback) as *const TransformCallback as *mut std::ffi::c_void,
          )
        })
    } else {
      (None, std::ptr::null_mut())
    };
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, callback, context);
    }
  }

  pub(crate) fn set_editor_zoom(&self, zoom_percent: f64) {
    unsafe {
      screenwide_preview_surface_set_editor_zoom(self.handle, zoom_percent);
    }
  }

  pub(crate) fn center_editor(&self) {
    unsafe {
      screenwide_preview_surface_center_editor(self.handle);
    }
  }

  pub(crate) fn set_selection(&self, selection: Option<PreviewSelection>) {
    unsafe {
      screenwide_preview_surface_set_selection(
        self.handle,
        selection
          .as_ref()
          .map_or(std::ptr::null(), std::ptr::from_ref),
      );
    }
  }

  pub(crate) fn set_selection_visible(&self, visible: bool) {
    unsafe {
      screenwide_preview_surface_set_selection_visible(self.handle, i32::from(visible));
    }
  }

  pub(crate) fn set_selection_targets(&self, targets: Option<&[PreviewSelection]>) {
    unsafe {
      screenwide_preview_surface_set_selection_targets(
        self.handle,
        targets.map_or(std::ptr::null(), |targets| targets.as_ptr()),
        targets.map_or(0, <[PreviewSelection]>::len),
        i32::from(targets.is_some()),
      );
    }
  }

  pub(crate) fn set_selection_snapping(&self, enabled: bool) {
    unsafe {
      screenwide_preview_surface_set_selection_snapping(self.handle, i32::from(enabled));
    }
  }

  pub(crate) fn set_selection_callback(&mut self, callback: SelectionCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut SelectionCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_set_selection_callback(
        self.handle,
        Some(selection_callback),
        context,
      );
    }
    release_callback_on_main(self.selection_callback.replace(callback));
  }

  /// Installs the native selection-body gesture callback. The callback is
  /// invoked on the main thread with normalized movement from gesture start.
  /// Keeping it here, beside the existing transform callback, lets the
  /// frontend mirror the native gesture without routing pointer movement
  /// through the webview.
  pub(crate) fn set_selection_gesture_callback(&mut self, callback: SelectionGestureCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut SelectionGestureCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_set_selection_gesture_callback(
        self.handle,
        Some(selection_gesture_callback),
        context,
      );
    }
    release_callback_on_main(self.selection_gesture_callback.replace(callback));
  }
}
