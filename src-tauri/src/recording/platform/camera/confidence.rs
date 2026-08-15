// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{ffi::c_void, sync::mpsc, thread::JoinHandle};

use cidre::{arc, cg, cv, vt};

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
        while let Ok(frame) = receiver.recv() {
          if let Ok((width, height, rgba)) = thumbnail(&frame.0) {
            monitor.send_camera(width, height, rgba);
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

fn thumbnail(buffer: &cv::PixelBuf) -> Result<(u16, u16, Vec<u8>), String> {
  let source_width = buffer.width();
  let source_height = buffer.height();
  if source_width == 0 || source_height == 0 {
    return Err("The camera confidence frame is empty".to_owned());
  }
  let scale = (MAX_WIDTH as f64 / source_width as f64)
    .min(MAX_HEIGHT as f64 / source_height as f64)
    .min(1.0);
  let width = ((source_width as f64 * scale).round() as usize).max(1);
  let height = ((source_height as f64 * scale).round() as usize).max(1);
  let stride = width * 4;
  let mut pixels = vec![0_u8; stride * height];
  let image = vt::cg_image_from_cv_pixel_buf(buffer, None).map_err(|error| error.to_string())?;
  let color_space = unsafe { CGColorSpaceCreateDeviceRGB() };
  if color_space.is_null() {
    return Err("Core Graphics could not create a camera color space".to_owned());
  }
  let context = unsafe {
    CGBitmapContextCreate(
      pixels.as_mut_ptr().cast(),
      width,
      height,
      8,
      stride,
      color_space,
      0x2002,
    )
  };
  unsafe { CGColorSpaceRelease(color_space) };
  if context.is_null() {
    return Err("Core Graphics could not create a camera thumbnail".to_owned());
  }
  unsafe {
    CGContextDrawImage(
      context,
      cg::Rect::new(0.0, 0.0, width as f64, height as f64),
      &image,
    );
    CGContextRelease(context);
  }
  for pixel in pixels.chunks_exact_mut(4) {
    pixel.swap(0, 2);
  }
  Ok((width as u16, height as u16, pixels))
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
  fn CGBitmapContextCreate(
    data: *mut c_void,
    width: usize,
    height: usize,
    bits_per_component: usize,
    bytes_per_row: usize,
    color_space: *const c_void,
    bitmap_info: u32,
  ) -> *mut c_void;
  fn CGColorSpaceCreateDeviceRGB() -> *const c_void;
  fn CGColorSpaceRelease(color_space: *const c_void);
  fn CGContextDrawImage(context: *mut c_void, rect: cg::Rect, image: &cg::Image);
  fn CGContextRelease(context: *mut c_void);
}
