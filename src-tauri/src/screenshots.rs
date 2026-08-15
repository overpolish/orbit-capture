// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

pub(crate) mod encoding;
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod mesh;
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod mesh_gpu;
mod output;
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

pub(crate) use crate::capture_geometry::physical_capture_rect;
#[cfg(test)]
pub(crate) use crate::capture_geometry::CaptureRect;
pub use encoding::encode_png;
#[cfg(not(target_os = "macos"))]
pub use encoding::rounded_corners;
pub(crate) use mesh::validate_mesh;
#[cfg(all(test, target_os = "macos"))]
pub(crate) use mesh::MeshGradientPoint;
pub use output::{compose_screenshot, ScreenshotOutputSettings};
pub(crate) use output::{output_dimensions, output_placement, parse_hex_colour};
#[cfg(target_os = "macos")]
pub(crate) use platform::{compose_output_layers, native_canvas, NativeCanvas, StillOverlay};

/// A captured still: straight (non-premultiplied) RGBA8, packed rows, top down.
/// That is what both the clipboard and the PNG encoder want.
#[derive(Clone)]
pub struct CapturedImage {
  pub rgba: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn validate_output_settings(
  source_width: u32,
  source_height: u32,
  settings: &ScreenshotOutputSettings,
) -> Result<(), String> {
  output_placement(source_width, source_height, settings)?;
  parse_hex_colour(&settings.background_color)?;
  if !settings.radius_percent.is_finite()
    || !(0.0..=50.0).contains(&settings.radius_percent)
    || !settings.background_radius_percent.is_finite()
    || !(0.0..=50.0).contains(&settings.background_radius_percent)
  {
    return Err("The output corner radius is not valid".to_owned());
  }
  match settings.background_type.as_str() {
    "solid" => Ok(()),
    "mesh" => mesh::validate_mesh(
      &settings.mesh_colors,
      &settings.mesh_points,
      settings.mesh_warp_percent,
    ),
    _ => Err("The output background type is not valid".to_owned()),
  }
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

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScreenshotDestination {
  Export,
  #[default]
  Clipboard,
  Both,
}

/// The naming macOS's own `screencapture` uses, which is the least surprising
/// thing to find sitting on a Desktop. Recordings are named the same way, from
/// the moment they started, so a session's files sit together in order.
pub fn capture_file_stem(captured_at: NaiveDateTime) -> String {
  captured_at
    .format("Screenwide %Y-%m-%d at %H.%M.%S")
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

pub(crate) async fn capture(
  app: &AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
) -> Result<CapturedImage, String> {
  // Read as the shutter fires, the same way a recording reads it as it starts.
  let include_own_windows = crate::settings::current(app).record_screenwide_windows;

  #[cfg(target_os = "macos")]
  {
    tauri::async_runtime::spawn_blocking(move || {
      platform::capture_blocking(target, include_own_windows, show_cursor)
    })
    .await
    .map_err(|error| error.to_string())?
  }

  #[cfg(target_os = "windows")]
  {
    let _ = include_own_windows;
    tauri::async_runtime::spawn_blocking(move || platform_windows::capture(target, show_cursor))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  {
    let _ = (include_own_windows, target, show_cursor);
    Err("Screenshots are not available on this platform".to_owned())
  }
}

pub(crate) async fn capture_for_text_recognition(
  app: &AppHandle,
  target: ScreenshotTarget,
  excluded_window_ids: &[u32],
) -> Result<CapturedImage, String> {
  let _ = app;

  #[cfg(target_os = "macos")]
  {
    let excluded_window_ids = excluded_window_ids.to_vec();
    tauri::async_runtime::spawn_blocking(move || {
      platform::capture_for_text_recognition_blocking(target, &excluded_window_ids)
    })
    .await
    .map_err(|error| error.to_string())?
  }

  #[cfg(target_os = "windows")]
  {
    let _ = excluded_window_ids;
    capture(app, target, false).await
  }

  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  {
    let _ = (target, excluded_window_ids);
    Err("Text recognition is not available on this platform".to_owned())
  }
}

/// Captures a still and either copies it or saves it, returning the path it was
/// written to when it went to disk.
#[tauri::command]
pub async fn capture_still(
  app: AppHandle,
  target: ScreenshotTarget,
  show_cursor: bool,
  destination: ScreenshotDestination,
) -> Result<Option<PathBuf>, String> {
  crate::text_recognition::dismiss(&app);
  let image = capture(&app, target, show_cursor).await?;

  if matches!(
    destination,
    ScreenshotDestination::Clipboard | ScreenshotDestination::Both
  ) {
    // The clipboard takes the raw pixels, so there is nothing to encode.
    app
      .clipboard()
      .write_image(&Image::new(&image.rgba, image.width, image.height))
      .map_err(|error| error.to_string())?;
    if matches!(destination, ScreenshotDestination::Clipboard) {
      let _ = crate::windows::hide_recording_ui(app.clone());
      return Ok(None);
    }
  }

  // With the clipboard off, the export window takes over: the user names the
  // file and picks where it goes, so nothing is written here.
  crate::exports::present_screenshot(&app, image, capture_file_stem(Local::now().naive_local()))?;

  Ok(None)
}
