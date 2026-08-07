use std::path::{Path, PathBuf};

use image::DynamicImage;
use rayon::prelude::*;
use serde::Serialize;
use tauri::{AppHandle, Manager};

mod platform;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDetails {
  id: u32,
  name: String,
  layout_position: Position,
  layout_size: Size,
  position: Position,
  physical_position: Position,
  physical_size: Size,
  size: Size,
  scale_factor: f32,
  is_primary: bool,
  is_builtin: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Position {
  x: i32,
  y: i32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Size {
  width: u32,
  height: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowDetails {
  id: u32,
  pid: u32,
  app_name: String,
  title: String,
  position: Position,
  size: Size,
  app_icon_path: Option<PathBuf>,
  thumbnail_path: Option<PathBuf>,
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Result<Vec<MonitorDetails>, String> {
  let capture_monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
  let tauri_monitors = app
    .available_monitors()
    .map_err(|error| error.to_string())?;

  if capture_monitors.len() != tauri_monitors.len() {
    return Err("Tauri and xcap returned different monitor counts".into());
  }

  // Tauri does not expose a capture API identifier, so, as in Orbit Cursor,
  // monitor ordering is the only cross-API mapping available on both platforms.
  capture_monitors
    .into_iter()
    .zip(tauri_monitors)
    .map(|(monitor, tauri_monitor)| {
      let scale_factor = tauri_monitor.scale_factor();
      let physical_position = tauri_monitor.position();
      let physical_size = tauri_monitor.size();
      let logical_position = physical_position.to_logical::<f64>(scale_factor);
      let logical_size = physical_size.to_logical::<f64>(scale_factor);

      Ok(MonitorDetails {
        id: monitor.id().map_err(|error| error.to_string())?,
        name: monitor
          .friendly_name()
          .or_else(|_| monitor.name())
          .map_err(|error| error.to_string())?,
        layout_position: Position {
          x: monitor.x().map_err(|error| error.to_string())?,
          y: monitor.y().map_err(|error| error.to_string())?,
        },
        layout_size: Size {
          width: monitor.width().map_err(|error| error.to_string())?,
          height: monitor.height().map_err(|error| error.to_string())?,
        },
        position: Position {
          x: logical_position.x.round() as i32,
          y: logical_position.y.round() as i32,
        },
        physical_position: Position {
          x: physical_position.x,
          y: physical_position.y,
        },
        physical_size: Size {
          width: physical_size.width,
          height: physical_size.height,
        },
        size: Size {
          width: logical_size.width.round() as u32,
          height: logical_size.height.round() as u32,
        },
        scale_factor: scale_factor as f32,
        is_primary: monitor.is_primary().map_err(|error| error.to_string())?,
        is_builtin: monitor.is_builtin().map_err(|error| error.to_string())?,
      })
    })
    .collect()
}

#[tauri::command]
pub async fn list_windows(app: AppHandle) -> Result<Vec<WindowDetails>, String> {
  let cache_dir = app
    .path()
    .temp_dir()
    .map_err(|error| error.to_string())?
    .join("OrbitCapture")
    .join("window-selector");
  tauri::async_runtime::spawn_blocking(move || enumerate_windows(&cache_dir))
    .await
    .map_err(|error| error.to_string())?
}

fn enumerate_windows(cache_dir: &Path) -> Result<Vec<WindowDetails>, String> {
  std::fs::create_dir_all(cache_dir).map_err(|error| error.to_string())?;
  let current_pid = std::process::id();
  let windows = xcap::Window::all().map_err(|error| error.to_string())?;
  let selectable_window_ids = platform::selectable_window_ids();

  let mut details = windows
    .into_par_iter()
    .filter_map(|window| {
      let id = window.id().ok()?;
      let pid = window.pid().ok()?;
      let app_name = window.app_name().ok()?;
      let title = window.title().ok()?;
      let width = window.width().ok()?;
      let height = window.height().ok()?;

      if pid == current_pid
        || selectable_window_ids
          .as_ref()
          .is_some_and(|window_ids| !window_ids.contains(&id))
        || title.trim().is_empty()
        || width == 0
        || height == 0
        || window.is_minimized().unwrap_or(true)
      {
        return None;
      }

      let thumbnail_path = create_thumbnail(&window, cache_dir, id);
      let app_icon_path = platform::app_icon(cache_dir, pid);

      Some(WindowDetails {
        id,
        pid,
        app_name,
        title,
        position: Position {
          x: window.x().ok()?,
          y: window.y().ok()?,
        },
        size: Size { width, height },
        app_icon_path,
        thumbnail_path,
      })
    })
    .collect::<Vec<_>>();

  details.sort_by_cached_key(|window| {
    (
      window.app_name.to_lowercase(),
      window.title.to_lowercase(),
      window.id,
    )
  });

  Ok(details)
}

fn create_thumbnail(window: &xcap::Window, cache_dir: &Path, id: u32) -> Option<PathBuf> {
  let path = cache_dir.join(format!("window-{id}.png"));
  let image = window.capture_image().ok()?;
  DynamicImage::ImageRgba8(image)
    .thumbnail(320, 180)
    .save(&path)
    .ok()?;
  Some(path)
}

#[tauri::command]
pub async fn resize_window(
  id: u32,
  pid: u32,
  title: String,
  width: u32,
  height: u32,
) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || {
    platform::resize_window(id, pid, &title, width, height)
  })
  .await
  .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn center_window(id: u32, pid: u32, title: String) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || platform::center_window(id, pid, &title))
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn make_window_borderless(id: u32, pid: u32, title: String) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  {
    tauri::async_runtime::spawn_blocking(move || platform::make_borderless(id, pid, &title))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(not(target_os = "windows"))]
  {
    let _ = (id, pid, title);
    Err("Border controls are only available on Windows".into())
  }
}

#[tauri::command]
pub async fn restore_window_border(id: u32, pid: u32, title: String) -> Result<(), String> {
  #[cfg(target_os = "windows")]
  {
    tauri::async_runtime::spawn_blocking(move || platform::restore_border(id, pid, &title))
      .await
      .map_err(|error| error.to_string())?
  }

  #[cfg(not(target_os = "windows"))]
  {
    let _ = (id, pid, title);
    Err("Border controls are only available on Windows".into())
  }
}
