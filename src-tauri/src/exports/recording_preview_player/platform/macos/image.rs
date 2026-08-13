// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::c_void;

use cidre::{cg, cm};

use crate::screenshots::CapturedImage;

pub(super) fn captured_image(image: &cg::Image) -> Result<CapturedImage, String> {
  let width = image.width();
  let height = image.height();
  let stride = width * 4;
  let mut rgba = vec![0_u8; stride * height];
  let colour_space = unsafe { CGColorSpaceCreateDeviceRGB() };
  if colour_space.is_null() {
    return Err("Core Graphics could not create a preview colour space".to_owned());
  }
  let context = unsafe {
    CGBitmapContextCreate(
      rgba.as_mut_ptr().cast(),
      width,
      height,
      8,
      stride,
      colour_space,
      0x2002,
    )
  };
  unsafe { CGColorSpaceRelease(colour_space) };
  if context.is_null() {
    return Err("Core Graphics could not create a preview bitmap".to_owned());
  }
  unsafe {
    CGContextDrawImage(
      context,
      cg::Rect::new(0.0, 0.0, width as f64, height as f64),
      image,
    );
    CGContextRelease(context);
  }
  for pixel in rgba.chunks_exact_mut(4) {
    pixel.swap(0, 2);
  }
  Ok(CapturedImage {
    height: height as u32,
    rgba,
    width: width as u32,
  })
}

pub(super) fn frame_position(requested_ms: u64, duration_ms: u64) -> u64 {
  requested_ms.min(duration_ms.saturating_sub(1))
}

pub(super) fn time(milliseconds: u64) -> cm::Time {
  cm::Time::new(milliseconds as i64, 1_000)
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
  fn CGBitmapContextCreate(
    data: *mut c_void,
    width: usize,
    height: usize,
    bits_per_component: usize,
    bytes_per_row: usize,
    colour_space: *const c_void,
    bitmap_info: u32,
  ) -> *mut c_void;
  fn CGColorSpaceCreateDeviceRGB() -> *const c_void;
  fn CGColorSpaceRelease(colour_space: *const c_void);
  fn CGContextDrawImage(context: *mut c_void, rect: cg::Rect, image: &cg::Image);
  fn CGContextRelease(context: *mut c_void);
}
