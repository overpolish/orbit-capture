// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistent macOS scrub output.
//!
//! Unlike an `AVAssetReader`, `AVPlayerItemVideoOutput` owns a long-lived decode
//! and frame-cache context. That makes arbitrary forward and backward timeline
//! movement use the same player state instead of reopening a GOP for every drag.

use std::{ffi::CString, path::Path};

unsafe extern "C" {
  fn orbit_preview_scrubber_create(
    path: *const std::ffi::c_char,
    width: u32,
    height: u32,
  ) -> *mut std::ffi::c_void;
  fn orbit_preview_scrubber_copy_frame(
    handle: *mut std::ffi::c_void,
    milliseconds: i64,
    rough: i32,
    width: *mut u32,
    height: *mut u32,
  ) -> *mut std::ffi::c_void;
  fn orbit_preview_pixel_buffer_release(pixels: *mut std::ffi::c_void);
  fn orbit_preview_scrubber_resize(handle: *mut std::ffi::c_void, width: u32, height: u32) -> i32;
  fn orbit_preview_scrubber_destroy(handle: *mut std::ffi::c_void);
}

pub(super) struct NativePixelFrame {
  handle: *mut std::ffi::c_void,
  pub(super) height: u32,
  pub(super) width: u32,
}

impl NativePixelFrame {
  pub(super) fn as_ptr(&self) -> *mut std::ffi::c_void {
    self.handle
  }
}

impl Drop for NativePixelFrame {
  fn drop(&mut self) {
    unsafe { orbit_preview_pixel_buffer_release(self.handle) };
  }
}

unsafe impl Send for NativePixelFrame {}

pub(super) struct NativeFrameScrubber {
  handle: *mut std::ffi::c_void,
}

impl NativeFrameScrubber {
  pub(super) fn open(path: &Path, width: u32, height: u32) -> Result<Self, String> {
    let path = CString::new(path.to_string_lossy().as_bytes())
      .map_err(|_| "The recording path contains an invalid character".to_owned())?;
    let handle = unsafe { orbit_preview_scrubber_create(path.as_ptr(), width, height) };
    if handle.is_null() {
      Err("AVFoundation could not open the recording scrubber".to_owned())
    } else {
      Ok(Self { handle })
    }
  }

  /// Both live and settled requests seek exact frames on the persistent player.
  /// Broad seek tolerance makes a drag collapse onto nearby keyframes.
  pub(super) fn frame_at(&self, position_ms: u64, rough: bool) -> Result<NativePixelFrame, String> {
    let mut width = 0;
    let mut height = 0;
    let handle = unsafe {
      orbit_preview_scrubber_copy_frame(
        self.handle,
        position_ms as i64,
        i32::from(rough),
        &mut width,
        &mut height,
      )
    };
    if handle.is_null() || width == 0 || height == 0 {
      return Err("AVFoundation could not produce the requested scrub frame".to_owned());
    }
    Ok(NativePixelFrame {
      handle,
      height,
      width,
    })
  }
}

impl NativeFrameScrubber {
  /// Re-aims the persistent player at a new output size without paying the
  /// player construction cost. Returns false when the output swap failed and
  /// the scrubber must be recreated.
  pub(super) fn resize(&self, width: u32, height: u32) -> bool {
    unsafe { orbit_preview_scrubber_resize(self.handle, width, height) != 0 }
  }
}

impl Drop for NativeFrameScrubber {
  fn drop(&mut self) {
    unsafe { orbit_preview_scrubber_destroy(self.handle) };
  }
}

unsafe impl Send for NativeFrameScrubber {}
