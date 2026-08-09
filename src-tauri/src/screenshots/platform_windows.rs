// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::screenshots::{physical_capture_rect, CapturedImage, ScreenshotTarget};

fn monitor(monitor_id: u32) -> Result<xcap::Monitor, String> {
  xcap::Monitor::all()
    .map_err(|error| error.to_string())?
    .into_iter()
    .find(|monitor| monitor.id().ok() == Some(monitor_id))
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())
}

fn captured(image: image::RgbaImage) -> CapturedImage {
  CapturedImage {
    width: image.width(),
    height: image.height(),
    rgba: image.into_raw(),
  }
}

pub fn capture(target: ScreenshotTarget, show_cursor: bool) -> Result<CapturedImage, String> {
  // Windows Graphics Capture cannot draw the pointer into a frame, so the
  // bar's "Show cursor" toggle has no effect on stills here. macOS honours it
  // through ScreenCaptureKit. This divergence is a platform limit, not a bug,
  // and is why the toggle is not disabled on Windows: it still governs
  // recordings.
  let _ = show_cursor;

  match target {
    ScreenshotTarget::Screen { monitor_id } => monitor(monitor_id)?
      .capture_image()
      .map(captured)
      .map_err(|error| error.to_string()),
    ScreenshotTarget::Region { monitor_id, region } => {
      let monitor = monitor(monitor_id)?;
      let scale = f64::from(monitor.scale_factor().map_err(|error| error.to_string())?);
      // xcap reports Windows monitors in device pixels already, unlike macOS
      // where it reports points, so these need no scaling of their own.
      let width = monitor.width().map_err(|error| error.to_string())?;
      let height = monitor.height().map_err(|error| error.to_string())?;
      let rect = physical_capture_rect(region, scale, width, height)
        .ok_or_else(|| "The selected region is not on the monitor".to_owned())?;

      monitor
        .capture_region(rect.x, rect.y, rect.width, rect.height)
        .map(captured)
        .map_err(|error| error.to_string())
    }
    ScreenshotTarget::Window { window_id } => xcap::Window::all()
      .map_err(|error| error.to_string())?
      .into_iter()
      .find(|window| window.id().ok() == Some(window_id))
      .ok_or_else(|| "The selected window is no longer available".to_owned())?
      .capture_image()
      .map(captured)
      .map_err(|error| error.to_string()),
  }
}
