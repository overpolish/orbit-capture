// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

pub(crate) mod encoding;
mod mesh;
mod mesh_gpu;
mod output;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "windows")]
mod platform_windows;
#[cfg(test)]
mod tests;

use std::{
  path::{Path, PathBuf},
  sync::{Arc, Condvar, Mutex, OnceLock},
};

use chrono::{Local, NaiveDateTime};
use image::codecs::jpeg::JpegEncoder;
use serde::Deserialize;
use tauri::{
  image::Image,
  ipc::{Channel, InvokeResponseBody},
  AppHandle, Manager,
};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::Region;

pub(crate) use crate::capture_geometry::physical_capture_rect;
#[cfg(test)]
pub(crate) use crate::capture_geometry::CaptureRect;
pub use encoding::{encode_png, rounded_corners};
pub use output::{compose_screenshot, ScreenshotOutputSettings};

/// A captured still: straight (non-premultiplied) RGBA8, packed rows, top down.
/// That is what both the clipboard and the PNG encoder want.
#[derive(Clone)]
pub struct CapturedImage {
  pub rgba: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

struct MeshPreviewRequest {
  channel: Channel,
  colors: Vec<String>,
  height: u32,
  points: Vec<mesh::MeshGradientPoint>,
  request_id: u32,
  seed: u32,
  warp_percent: f64,
  width: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshPreviewOptions {
  colors: Vec<String>,
  height: u32,
  points: Vec<mesh::MeshGradientPoint>,
  request_id: u32,
  seed: u32,
  warp_percent: f64,
  width: u32,
}

#[derive(Default)]
pub struct MeshPreviewState {
  pending: Arc<(Mutex<Option<MeshPreviewRequest>>, Condvar)>,
  worker: OnceLock<()>,
}

impl MeshPreviewState {
  fn submit(&self, request: MeshPreviewRequest) -> Result<(), String> {
    let pending = Arc::clone(&self.pending);
    self.worker.get_or_init(|| {
      std::thread::Builder::new()
        .name("mesh-preview".to_owned())
        .spawn(move || mesh_preview_worker(&pending))
        .expect("the mesh preview worker must start");
    });
    let (slot, wake) = &*self.pending;
    *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request);
    wake.notify_one();
    Ok(())
  }
}

fn mesh_preview_worker(pending: &(Mutex<Option<MeshPreviewRequest>>, Condvar)) {
  loop {
    let request = {
      let (slot, wake) = pending;
      let mut slot = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
      while slot.is_none() {
        slot = wake
          .wait(slot)
          .unwrap_or_else(|poisoned| poisoned.into_inner());
      }
      slot.take().expect("a woken mesh worker has a request")
    };
    let Ok(canvas) = mesh::mesh_canvas(
      request.width,
      request.height,
      &request.colors,
      &request.points,
      request.seed,
      request.warp_percent,
    ) else {
      continue;
    };
    // If another request arrived while the GPU was working, skip the obsolete
    // frame entirely. The worker will pick up only the newest replacement.
    if pending
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .is_some()
    {
      continue;
    }
    let mut jpeg = Vec::new();
    if JpegEncoder::new_with_quality(&mut jpeg, 94)
      .encode_image(&canvas)
      .is_err()
    {
      continue;
    }
    // Encoding is outside the GPU work, so a newer request may have replaced
    // this frame in the meantime as well.
    if pending
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .is_some()
    {
      continue;
    }
    let mut payload = Vec::with_capacity(12 + jpeg.len());
    payload.extend_from_slice(&request.request_id.to_le_bytes());
    payload.extend_from_slice(&request.width.to_le_bytes());
    payload.extend_from_slice(&request.height.to_le_bytes());
    payload.extend_from_slice(&jpeg);
    let _ = request.channel.send(InvokeResponseBody::Raw(payload));
  }
}

/// Queues a native GPU frame for the export window. This uses the same binary
/// channel architecture as camera and recording previews; the command returns
/// immediately and only the newest completed request reaches the canvas.
#[tauri::command]
pub fn render_mesh_background_preview(
  state: tauri::State<'_, MeshPreviewState>,
  options: MeshPreviewOptions,
  channel: Channel,
) -> Result<(), String> {
  if options.width == 0 || options.height == 0 {
    return Err("The mesh preview dimensions are not valid".to_owned());
  }
  state.submit(MeshPreviewRequest {
    channel,
    colors: options.colors,
    height: options.height,
    points: options.points,
    request_id: options.request_id,
    seed: options.seed,
    warp_percent: options.warp_percent,
    width: options.width,
  })
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

pub(crate) async fn capture(
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
