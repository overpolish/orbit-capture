// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  ffi::{c_char, c_void},
  sync::mpsc,
  thread::JoinHandle,
};

use cidre::{arc, cv};

use crate::recording::monitor::RecordingMonitor;

const MAX_WIDTH: usize = 48;
const MAX_HEIGHT: usize = 30;

pub(super) struct CameraFrame(pub(super) arc::R<cv::PixelBuf>);

// SAFETY: the capture callback retains the pixel buffer, moves that ownership
// into this bounded channel and never accesses that retained reference again.
// Only this worker reads it afterwards.
unsafe impl Send for CameraFrame {}

pub(super) struct ConfidenceWorker {
  sender: Option<mpsc::SyncSender<CameraFrame>>,
  thread: Option<JoinHandle<()>>,
}

impl ConfidenceWorker {
  pub(super) fn spawn(monitor: std::sync::Arc<RecordingMonitor>) -> Result<Self, String> {
    // There is deliberately no backlog: while this worker scales one frame,
    // capture drops confidence frames and hands over the next current one as
    // soon as the worker is waiting again.
    let (sender, receiver) = mpsc::sync_channel::<CameraFrame>(0);
    let thread = std::thread::Builder::new()
      .name("screenwide-camera-confidence".to_owned())
      .spawn(move || {
        // A failed scaler still drains the channel: capture uses a rendezvous
        // send, so a receiver that stops listening would stall it.
        let scaler = match Scaler::create() {
          Ok(scaler) => Some(scaler),
          Err(error) => {
            eprintln!("The camera confidence thumbnail scaler is unavailable: {error}");
            None
          }
        };
        while let Ok(frame) = receiver.recv() {
          let Some(scaler) = scaler.as_ref() else {
            continue;
          };
          let Some((width, height)) = thumbnail_size(&frame.0) else {
            continue;
          };
          let mut pixels = vec![0_u8; usize::from(width) * usize::from(height) * 4];
          if scaler.thumbnail(&frame.0, width, height, &mut pixels) {
            monitor.send_camera(width, height, pixels);
          }
        }
      })
      .map_err(|error| error.to_string())?;
    Ok(Self {
      sender: Some(sender),
      thread: Some(thread),
    })
  }

  pub(super) fn sender(&self) -> mpsc::SyncSender<CameraFrame> {
    self.sender.as_ref().expect("worker is active").clone()
  }

  pub(super) fn stop(mut self) {
    self.sender.take();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

impl Drop for ConfidenceWorker {
  fn drop(&mut self) {
    self.sender.take();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

/// The thumbnail keeps the source aspect ratio inside the recording bar's
/// budget and never upscales a camera that is already smaller.
fn thumbnail_size(buffer: &cv::PixelBuf) -> Option<(u16, u16)> {
  let source_width = buffer.width();
  let source_height = buffer.height();
  if source_width == 0 || source_height == 0 {
    return None;
  }
  let scale = (MAX_WIDTH as f64 / source_width as f64)
    .min(MAX_HEIGHT as f64 / source_height as f64)
    .min(1.0);
  let width = ((source_width as f64 * scale).round() as u16).max(1);
  let height = ((source_height as f64 * scale).round() as u16).max(1);
  Some((width, height))
}

/// Owns the Metal device, pipelines and texture cache behind
/// `confidence_scaler_macos.m` so every exit path tears them down.
struct Scaler(*mut c_void);

impl Scaler {
  fn create() -> Result<Self, String> {
    let mut error = [0 as c_char; 256];
    let handle = unsafe { screenwide_confidence_scaler_create(error.as_mut_ptr(), error.len()) };
    if handle.is_null() {
      let bytes = error.map(|byte| byte as u8);
      let text = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
      return Err(String::from_utf8_lossy(text).into_owned());
    }
    Ok(Self(handle))
  }

  fn thumbnail(&self, buffer: &cv::PixelBuf, width: u16, height: u16, out: &mut [u8]) -> bool {
    if out.len() < usize::from(width) * usize::from(height) * 4 {
      return false;
    }
    unsafe {
      screenwide_confidence_scaler_thumbnail(
        self.0,
        buffer as *const cv::PixelBuf as *const c_void,
        width,
        height,
        out.as_mut_ptr(),
      )
    }
  }
}

impl Drop for Scaler {
  fn drop(&mut self) {
    unsafe { screenwide_confidence_scaler_destroy(self.0) };
  }
}

// SAFETY: the scaler is created and used on the confidence worker thread only;
// this marker exists so the handle may be moved into that thread's closure.
unsafe impl Send for Scaler {}

unsafe extern "C" {
  fn screenwide_confidence_scaler_create(
    error_text: *mut c_char,
    error_capacity: usize,
  ) -> *mut c_void;
  fn screenwide_confidence_scaler_thumbnail(
    scaler: *mut c_void,
    frame: *const c_void,
    width: u16,
    height: u16,
    out_rgba: *mut u8,
  ) -> bool;
  fn screenwide_confidence_scaler_destroy(scaler: *mut c_void);
}

#[cfg(test)]
mod tests {
  use super::*;
  use cidre::cf;

  /// CoreVideo only hands a Metal texture cache buffers backed by an IOSurface,
  /// which is what the camera itself delivers.
  fn pixel_buffer(width: usize, height: usize, format: cv::PixelFormat) -> arc::R<cv::PixelBuf> {
    let empty = cf::Dictionary::with_keys_values(&[], &[]).expect("an empty attribute dictionary");
    let attributes = cf::Dictionary::with_keys_values(
      &[cv::pixel_buffer::keys::io_surf_props().as_type_ref()],
      &[empty.as_type_ref()],
    )
    .expect("the pixel buffer attributes");
    cv::PixelBuf::new(width, height, format, Some(&attributes)).expect("a test pixel buffer")
  }

  /// The guard only borrows the buffer for its own lifetime, which leaves the
  /// plane accessors usable while the base address stays locked.
  struct Lock(*mut cv::PixelBuf);

  impl Drop for Lock {
    fn drop(&mut self) {
      let result = unsafe { (*self.0).unlock_lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) };
      assert!(result.is_ok(), "the test buffer unlocks");
    }
  }

  fn lock(buffer: &mut cv::PixelBuf) -> Lock {
    let result = unsafe { buffer.lock_base_addr(cv::pixel_buffer::LockFlags::DEFAULT) };
    assert!(result.is_ok(), "the test buffer locks");
    Lock(buffer)
  }

  #[test]
  fn scales_a_bgra_frame_to_the_thumbnail_size() {
    let scaler = Scaler::create().expect("a Metal thumbnail scaler");
    let mut buffer = pixel_buffer(640, 480, cv::PixelFormat::_32_BGRA);
    {
      let _lock = lock(&mut buffer);
      let stride = buffer.plane_bytes_per_row(0);
      let base = buffer.plane_base_address(0).cast_mut();
      for row in 0..480 {
        for column in 0..640 {
          // Blue, green, red, alpha as CoreVideo stores 32BGRA.
          let pixel = unsafe { base.add(row * stride + column * 4) };
          unsafe { pixel.copy_from_nonoverlapping([50_u8, 100, 200, 255].as_ptr(), 4) };
        }
      }
    }

    let (width, height) = thumbnail_size(&buffer).expect("a thumbnail size");
    assert_eq!((width, height), (40, 30));
    let mut pixels = vec![0_u8; usize::from(width) * usize::from(height) * 4];
    assert!(scaler.thumbnail(&buffer, width, height, &mut pixels));
    for pixel in pixels.chunks_exact(4) {
      assert_eq!(pixel[3], 255);
      for (actual, expected) in pixel[..3].iter().zip([200_u8, 100, 50]) {
        assert!(
          actual.abs_diff(expected) <= 2,
          "expected {expected} but the scaler produced {actual}"
        );
      }
    }
  }

  #[test]
  fn converts_a_video_range_biplanar_frame_to_rgba() {
    let scaler = Scaler::create().expect("a Metal thumbnail scaler");
    let mut buffer = pixel_buffer(640, 480, cv::PixelFormat::_420V);
    {
      let _lock = lock(&mut buffer);
      // BT.709 video range encoding of red 200, green 100, blue 50.
      let luma_stride = buffer.plane_bytes_per_row(0);
      let luma = buffer.plane_base_address(0).cast_mut();
      for row in 0..480 {
        unsafe { luma.add(row * luma_stride).write_bytes(117, 640) };
      }
      let chroma_stride = buffer.plane_bytes_per_row(1);
      let chroma = buffer.plane_base_address(1).cast_mut();
      for row in 0..240 {
        for column in 0..320 {
          let pixel = unsafe { chroma.add(row * chroma_stride + column * 2) };
          unsafe { pixel.copy_from_nonoverlapping([96_u8, 174].as_ptr(), 2) };
        }
      }
    }

    let (width, height) = thumbnail_size(&buffer).expect("a thumbnail size");
    let mut pixels = vec![0_u8; usize::from(width) * usize::from(height) * 4];
    assert!(scaler.thumbnail(&buffer, width, height, &mut pixels));
    for pixel in pixels.chunks_exact(4) {
      assert_eq!(pixel[3], 255);
      for (actual, expected) in pixel[..3].iter().zip([200_u8, 100, 50]) {
        assert!(
          actual.abs_diff(expected) <= 2,
          "expected {expected} but the scaler produced {actual}"
        );
      }
    }
  }

  #[test]
  fn rejects_an_unsupported_pixel_format() {
    let scaler = Scaler::create().expect("a Metal thumbnail scaler");
    let buffer = pixel_buffer(64, 64, cv::PixelFormat::_32_ARGB);
    let mut pixels = vec![0_u8; 48 * 30 * 4];
    assert!(!scaler.thumbnail(&buffer, 48, 30, &mut pixels));
  }
}
