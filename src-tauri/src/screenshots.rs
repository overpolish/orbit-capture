// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod encoding;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "windows")]
mod platform_windows;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use serde::Deserialize;
use tauri::{image::Image, AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::Region;

pub use encoding::{encode_png, rounded_corners};

/// A captured still: straight (non-premultiplied) RGBA8, packed rows, top down.
/// That is what both the clipboard and the PNG encoder want.
pub struct CapturedImage {
  pub rgba: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "kind"
)]
pub enum ScreenshotTarget {
  Screen { monitor_id: u32 },
  Window { window_id: u32 },
  Region { monitor_id: u32, region: Region },
}

/// A capture rectangle in physical device pixels, relative to its monitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureRect {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
}

/// Converts a logical, monitor-local region into physical device pixels.
///
/// The two platforms disagree about units - ScreenCaptureKit's source rect is
/// in points, xcap's capture region is in physical pixels - so everything is
/// normalised to physical here, exactly once, and the macOS caller divides back
/// down. Edges are rounded before the size is derived from them, so the
/// rectangle can never disagree with its own corners by a pixel, and the result
/// is clamped to the monitor because xcap errors on an out-of-bounds region
/// rather than trimming it.
pub fn physical_capture_rect(
  region: Region,
  scale: f64,
  monitor_width: u32,
  monitor_height: u32,
) -> Option<CaptureRect> {
  let edges = [
    region.position.x,
    region.position.y,
    region.size.width,
    region.size.height,
    scale,
  ];
  if !edges.iter().all(|edge| edge.is_finite()) || scale <= 0.0 {
    return None;
  }

  let monitor_width = f64::from(monitor_width);
  let monitor_height = f64::from(monitor_height);
  let left = (region.position.x * scale)
    .round()
    .clamp(0.0, monitor_width);
  let top = (region.position.y * scale)
    .round()
    .clamp(0.0, monitor_height);
  let right = ((region.position.x + region.size.width) * scale)
    .round()
    .clamp(0.0, monitor_width);
  let bottom = ((region.position.y + region.size.height) * scale)
    .round()
    .clamp(0.0, monitor_height);

  if right <= left || bottom <= top {
    return None;
  }

  Some(CaptureRect {
    x: left as u32,
    y: top as u32,
    width: (right - left) as u32,
    height: (bottom - top) as u32,
  })
}

/// The naming macOS's own `screencapture` uses, which is the least surprising
/// thing to find sitting on a Desktop. Recordings are named the same way, from
/// the moment they started, so a session's files sit together in order.
pub fn capture_file_stem(captured_at: NaiveDateTime) -> String {
  captured_at
    .format("Orbit Capture %Y-%m-%d at %H.%M.%S")
    .to_string()
}

/// Appends " (2)", " (3)" and so on until the name is free, as both platforms'
/// file managers do. `exists` is injected so the walk can be tested without
/// touching a disk.
pub fn unique_path(
  directory: &Path,
  stem: &str,
  extension: &str,
  exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
  let mut candidate = directory.join(format!("{stem}.{extension}"));
  let mut suffix = 1_u32;

  while exists(&candidate) {
    suffix += 1;
    candidate = directory.join(format!("{stem} ({suffix}).{extension}"));
  }

  candidate
}

/// Where a still goes when it is not going to the clipboard. Both are the
/// platform's own screenshot destination.
pub fn screenshot_directory(app: &AppHandle) -> Result<PathBuf, String> {
  let path = app.path();

  #[cfg(target_os = "macos")]
  let directory = path.desktop_dir().map_err(|error| error.to_string())?;

  #[cfg(not(target_os = "macos"))]
  let directory = path
    .picture_dir()
    .map_err(|error| error.to_string())?
    .join("Screenshots");

  Ok(directory)
}

async fn capture(
  app: &AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  let _ = app;

  #[cfg(target_os = "macos")]
  {
    tauri::async_runtime::spawn_blocking(move || platform::capture_blocking(target, show_cursor))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(target_os = "windows")]
  {
    tauri::async_runtime::spawn_blocking(move || platform_windows::capture(target, show_cursor))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  {
    let _ = (target, show_cursor);
    Err("Screenshots are not available on this platform".to_owned())
  }
}

/// Captures a still and either copies it or saves it, returning the path it was
/// written to when it went to disk.
#[tauri::command]
pub async fn capture_still(
  app: AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
  to_clipboard: bool,
) -> Result<Option<PathBuf>, String> {
  let image = capture(&app, target, show_cursor).await?;

  if to_clipboard {
    // The clipboard takes the raw pixels, so there is nothing to encode.
    app
      .clipboard()
      .write_image(&Image::new(&image.rgba, image.width, image.height))
      .map_err(|error| error.to_string())?;

    return Ok(None);
  }

  // With the clipboard off, the export window takes over: the user names the
  // file and picks where it goes, so nothing is written here.
  crate::exports::present_screenshot(&app, image, capture_file_stem(Local::now().naive_local()))?;

  Ok(None)
}
