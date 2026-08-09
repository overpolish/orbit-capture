// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cidre::{cv, sc};

use crate::capture_kit::{display_scale, monitor_geometry, our_windows};
use crate::screenshots::{physical_capture_rect, CapturedImage, ScreenshotTarget};

async fn capture_filtered(
  filter: &sc::ContentFilter,
  cfg: &sc::StreamCfg,
) -> Result<CapturedImage, String> {
  let mut buf = sc::ScreenshotManager::capture_sample_buf(filter, cfg)
    .await
    .map_err(|error| error.to_string())?;
  let image = buf
    .image_buf_mut()
    .ok_or_else(|| "The capture produced no image".to_owned())?;
  let width = image.width();
  let height = image.height();
  let stride = image.bytes_per_row();

  if width == 0 || height == 0 {
    return Err("The capture produced an empty image".to_owned());
  }

  let flags = cv::pixel_buffer::LockFlags::READ_ONLY;
  // SAFETY: the buffer stays locked for exactly the copy below, and every read
  // is bounded by the stride and height the buffer itself reports.
  unsafe { image.lock_base_addr(flags) }
    .result()
    .map_err(|error| error.to_string())?;
  let base = unsafe { image.base_address() } as *const u8;
  if base.is_null() {
    unsafe { image.unlock_lock_base_addr(flags) };
    return Err("The capture produced no pixels".to_owned());
  }

  // ScreenCaptureKit hands back BGRA with rows padded out to its own stride,
  // while the clipboard and the PNG encoder both want packed RGBA.
  let mut rgba = vec![0_u8; width * height * 4];
  for row in 0..height {
    let source = unsafe { std::slice::from_raw_parts(base.add(row * stride), width * 4) };
    let target = &mut rgba[row * width * 4..(row + 1) * width * 4];
    for (source, target) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
      target[0] = source[2];
      target[1] = source[1];
      target[2] = source[0];
      target[3] = source[3];
    }
  }
  unsafe { image.unlock_lock_base_addr(flags) };

  Ok(CapturedImage {
    rgba,
    width: width as u32,
    height: height as u32,
  })
}

/// ScreenCaptureKit deals in Objective-C objects, which are not `Send`, so the
/// whole conversation is confined to one blocking thread and only the finished
/// pixels travel back out.
pub fn capture_blocking(
  target: ScreenshotTarget,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(|error| error.to_string())?
    .block_on(capture(target, show_cursor))
}

async fn capture(target: ScreenshotTarget, show_cursor: bool) -> Result<CapturedImage, String> {
  let content = sc::ShareableContent::current()
    .await
    .map_err(|error| error.to_string())?;
  let mut cfg = sc::StreamCfg::new();
  cfg.set_shows_cursor(show_cursor);
  cfg.set_pixel_format(cv::PixelFormat::_32_BGRA);

  match target {
    ScreenshotTarget::Screen { monitor_id } => {
      let displays = content.displays();
      let display = displays
        .iter()
        .find(|display| display.display_id().0 == monitor_id)
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let (_, width, height) = monitor_geometry(monitor_id)?;
      cfg.set_width(width as usize);
      cfg.set_height(height as usize);

      let filter =
        sc::ContentFilter::with_display_excluding_windows(display, &our_windows(&content));
      capture_filtered(&filter, &cfg).await
    }
    ScreenshotTarget::Region { monitor_id, region } => {
      let displays = content.displays();
      let display = displays
        .iter()
        .find(|display| display.display_id().0 == monitor_id)
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let (scale, monitor_width, monitor_height) = monitor_geometry(monitor_id)?;
      let rect = physical_capture_rect(region, scale, monitor_width, monitor_height)
        .ok_or_else(|| "The selected region is not on the monitor".to_owned())?;

      // The source rect is in points, so the one physical rectangle both
      // platforms agree on is divided back down here - and only here.
      cfg.set_src_rect(cidre::cg::Rect::new(
        f64::from(rect.x) / scale,
        f64::from(rect.y) / scale,
        f64::from(rect.width) / scale,
        f64::from(rect.height) / scale,
      ));
      cfg.set_width(rect.width as usize);
      cfg.set_height(rect.height as usize);

      let filter =
        sc::ContentFilter::with_display_excluding_windows(display, &our_windows(&content));
      capture_filtered(&filter, &cfg).await
    }
    ScreenshotTarget::Window { window_id } => {
      let windows = content.windows();
      let window = windows
        .iter()
        .find(|window| window.id() == window_id)
        .ok_or_else(|| "The selected window is no longer available".to_owned())?;
      let frame = window.frame();
      let displays = content.displays();
      let scale = displays
        .iter()
        .find(|display| {
          let bounds = display.frame();
          let centre_x = frame.origin.x + frame.size.width / 2.0;
          let centre_y = frame.origin.y + frame.size.height / 2.0;
          centre_x >= bounds.origin.x
            && centre_x < bounds.origin.x + bounds.size.width
            && centre_y >= bounds.origin.y
            && centre_y < bounds.origin.y + bounds.size.height
        })
        .map_or(1.0, |display| display_scale(display.display_id().0));
      cfg.set_width((frame.size.width * scale).round() as usize);
      cfg.set_height((frame.size.height * scale).round() as usize);

      let filter = sc::ContentFilter::with_desktop_independent_window(window);
      capture_filtered(&filter, &cfg).await
    }
  }
}
