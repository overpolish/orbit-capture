// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::Duration;

use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use tauri::WindowEvent;
use tauri::{
  AppHandle, LogicalSize, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow,
};

use super::{geometry::monitor_with_most_overlap, platform, WindowLabel};

const RECORDING_DOCK_POSITION_FILE: &str = "recording-dock-position.json";
const RECORDING_DOCK_MIN_WIDTH: f64 = 198.0;
const RECORDING_DOCK_MAX_WIDTH: f64 = 320.0;
const RECORDING_DOCK_HEIGHT: f64 = 44.0;
const RECORDING_DOCK_TOP_GAP: f64 = 8.0;

#[cfg(target_os = "windows")]
static DOCK_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The pill's position, held against the work area of the monitor it was
/// dropped on rather than as absolute desktop coordinates, so it lands in the
/// same visual spot whichever monitor the recording bar is on.
///
/// The offset is in *logical* pixels. Physical pixels would move the pill twice
/// as far from the corner when a 2x display's offset is applied to a 1x one,
/// and a proportional fraction would distort placement for a fixed-size window
/// that is meant to sit a fixed distance from an edge.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
struct RecordingDockOffset {
  x: f64,
  y: f64,
}

/// Where the user dropped the pill, relative to the work area of the monitor
/// they dropped it on. `None` means it has never been dragged.
static RECORDING_DOCK_OFFSET: Mutex<Option<RecordingDockOffset>> = Mutex::new(None);
/// The position the pill was last placed at programmatically, so that a plain
/// click on its buttons is never mistaken for a drag.
static RECORDING_DOCK_PLACED: Mutex<Option<PhysicalPosition<i32>>> = Mutex::new(None);

fn recording_dock_offset_path(app: &AppHandle) -> tauri::Result<PathBuf> {
  Ok(
    app
      .path()
      .app_config_dir()?
      .join(RECORDING_DOCK_POSITION_FILE),
  )
}

fn load_recording_dock_offset(app: &AppHandle) {
  let offset = recording_dock_offset_path(app)
    .ok()
    .and_then(|path| std::fs::read(path).ok())
    .and_then(|contents| serde_json::from_slice::<RecordingDockOffset>(&contents).ok());

  *RECORDING_DOCK_OFFSET
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = offset;
}

fn store_recording_dock_offset(app: &AppHandle, offset: RecordingDockOffset) -> tauri::Result<()> {
  *RECORDING_DOCK_OFFSET
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(offset);

  let path = recording_dock_offset_path(app)?;
  if let Some(directory) = path.parent() {
    std::fs::create_dir_all(directory)?;
  }
  let contents = serde_json::to_vec_pretty(&offset).map_err(std::io::Error::other)?;
  std::fs::write(path, contents)?;

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  load_recording_dock_offset(app);
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) {
    platform::initialize_recording_dock(&window)?;
  }

  Ok(())
}

/// The pill follows the recording bar rather than the recorded screen: it is
/// excluded from capture, so it never has to sit on the target monitor, and
/// following the bar puts it where the user is already looking.
fn recording_dock_monitor(app: &AppHandle) -> tauri::Result<Option<Monitor>> {
  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    // Geometry rather than `current_monitor`, because the bar is already
    // hidden by the time the pill is shown.
    if let Some(monitor) = monitor_with_most_overlap(app, &bar)? {
      return Ok(Some(monitor));
    }
  }

  app.primary_monitor()
}

/// Places the pill inside a work area: at its saved offset when it has one,
/// otherwise top-centre with a small gap. Always clamped so it stays wholly
/// inside, which is what makes a saved offset survive a move to a smaller
/// monitor.
fn recording_dock_local_position(
  work_area_size: PhysicalSize<u32>,
  dock_size: PhysicalSize<u32>,
  scale: f64,
  offset: Option<RecordingDockOffset>,
) -> (i32, i32) {
  let max_x = f64::from(work_area_size.width.saturating_sub(dock_size.width));
  let max_y = f64::from(work_area_size.height.saturating_sub(dock_size.height));
  let (x, y) = match offset {
    // Offsets are stored in logical pixels, so a pill dropped 200pt from the
    // corner of a Retina display lands 200pt from the corner of a 1x one.
    Some(offset) => (offset.x * scale, offset.y * scale),
    None => (max_x / 2.0, RECORDING_DOCK_TOP_GAP * scale),
  };

  (
    x.clamp(0.0, max_x).round() as i32,
    y.clamp(0.0, max_y).round() as i32,
  )
}

fn recording_dock_position(
  monitor: &Monitor,
  dock_size: PhysicalSize<u32>,
  offset: Option<RecordingDockOffset>,
) -> PhysicalPosition<i32> {
  let scale = monitor.scale_factor();
  let work_area = monitor.work_area();
  let (x, y) = recording_dock_local_position(work_area.size, dock_size, scale, offset);

  PhysicalPosition {
    x: work_area.position.x + x,
    y: work_area.position.y + y,
  }
}

pub fn show_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  load_recording_dock_offset(app);
  let dock = app
    .get_webview_window(WindowLabel::RecordingDock.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  if let Some(monitor) = recording_dock_monitor(app)? {
    let offset = *RECORDING_DOCK_OFFSET
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let scale = monitor.scale_factor();
    let logical_size = dock.outer_size()?.to_logical::<f64>(dock.scale_factor()?);
    let dock_size = PhysicalSize::new(
      (logical_size.width * scale).round() as u32,
      (RECORDING_DOCK_HEIGHT * scale).round() as u32,
    );
    let position = recording_dock_position(&monitor, dock_size, offset);
    dock.set_position(position)?;
    *RECORDING_DOCK_PLACED
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
  }

  platform::show(&dock)?;
  platform::restore_recording_level(&dock)
}

#[tauri::command]
pub fn resize_recording_dock(app: AppHandle, width: f64) -> Result<(), String> {
  if !width.is_finite() {
    return Err("The recording pill width must be finite".to_owned());
  }

  let to_message = |error: tauri::Error| error.to_string();
  let dock = app
    .get_webview_window(WindowLabel::RecordingDock.as_str())
    .ok_or_else(|| "The recording pill is unavailable".to_owned())?;
  let visible = dock.is_visible().map_err(to_message)?;
  let old_position = dock.outer_position().map_err(to_message)?;
  let old_size = dock.outer_size().map_err(to_message)?;
  dock
    .set_size(LogicalSize::new(
      width
        .ceil()
        .clamp(RECORDING_DOCK_MIN_WIDTH, RECORDING_DOCK_MAX_WIDTH),
      RECORDING_DOCK_HEIGHT,
    ))
    .map_err(to_message)?;

  if visible {
    let new_size = dock.outer_size().map_err(to_message)?;
    let centred_position = PhysicalPosition::new(
      old_position.x + (old_size.width as i32 - new_size.width as i32) / 2,
      old_position.y,
    );
    dock.set_position(centred_position).map_err(to_message)?;
    contain_recording_dock(&app, &dock).map_err(to_message)?;
    let position = dock.outer_position().map_err(to_message)?;
    *RECORDING_DOCK_PLACED
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
  }

  Ok(())
}

/// Clamps the pill into the work area it mostly sits on, so a drag cannot park
/// it under the menu bar, the notch or the taskbar.
fn contain_recording_dock(app: &AppHandle, dock: &WebviewWindow) -> tauri::Result<()> {
  let dock_position = dock.outer_position()?;
  let dock_size = dock.outer_size()?;
  let Some(monitor) = monitor_with_most_overlap(app, dock)? else {
    return Ok(());
  };
  let work_area = monitor.work_area();
  let max_x = work_area.position.x + work_area.size.width.saturating_sub(dock_size.width) as i32;
  let max_y = work_area.position.y + work_area.size.height.saturating_sub(dock_size.height) as i32;
  let contained = PhysicalPosition::new(
    dock_position
      .x
      .clamp(work_area.position.x, max_x.max(work_area.position.x)),
    dock_position
      .y
      .clamp(work_area.position.y, max_y.max(work_area.position.y)),
  );

  if contained != dock_position {
    dock.set_position(contained)?;
  }

  Ok(())
}

/// The pill's position expressed against the work area it was dropped on.
fn recording_dock_offset(
  app: &AppHandle,
  dock: &WebviewWindow,
) -> tauri::Result<Option<RecordingDockOffset>> {
  let Some(monitor) = monitor_with_most_overlap(app, dock)? else {
    return Ok(None);
  };
  let scale = monitor.scale_factor();
  let work_area = monitor.work_area();
  let dock_position = dock.outer_position()?;
  let dock_size = dock.outer_size()?;
  let max_x = f64::from(work_area.size.width.saturating_sub(dock_size.width));
  let max_y = f64::from(work_area.size.height.saturating_sub(dock_size.height));

  Ok(Some(RecordingDockOffset {
    x: f64::from(dock_position.x - work_area.position.x).clamp(0.0, max_x) / scale,
    y: f64::from(dock_position.y - work_area.position.y).clamp(0.0, max_y) / scale,
  }))
}

#[tauri::command]
pub fn finish_recording_dock_drag(app: AppHandle) -> Result<(), String> {
  let to_message = |error: tauri::Error| error.to_string();
  let dock = app
    .get_webview_window(WindowLabel::RecordingDock.as_str())
    .ok_or_else(|| "The recording pill is unavailable".to_owned())?;
  contain_recording_dock(&app, &dock).map_err(to_message)?;

  let position = dock.outer_position().map_err(to_message)?;
  let placed = *RECORDING_DOCK_PLACED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  // Pointer-up also fires for a plain click on the pill's buttons. Persisting
  // then would turn "never dragged" into a saved offset, and the pill would
  // stop using the default placement on whichever monitor the bar is on.
  if placed == Some(position) {
    return Ok(());
  }

  *RECORDING_DOCK_PLACED
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(position);
  if let Some(offset) = recording_dock_offset(&app, &dock).map_err(to_message)? {
    store_recording_dock_offset(&app, offset).map_err(to_message)?;
  }

  Ok(())
}

/// Windows does not deliver a pointer-up to the webview after a native drag,
/// so the pill's position is committed the same way the bar's is.
#[cfg(target_os = "windows")]
pub fn manage_recording_dock_movement(app: &AppHandle) {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) else {
    return;
  };
  let app = app.clone();

  window.on_window_event(move |event| {
    if !matches!(event, WindowEvent::Moved(_)) {
      return;
    }

    watch_for_recording_dock_mouse_up(app.clone());
  });
}

#[cfg(not(target_os = "windows"))]
pub fn manage_recording_dock_movement(_app: &AppHandle) {}

#[cfg(target_os = "windows")]
fn watch_for_recording_dock_mouse_up(app: AppHandle) {
  use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

  if DOCK_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
    return;
  }

  tauri::async_runtime::spawn_blocking(move || {
    loop {
      let is_pressed = unsafe { GetAsyncKeyState(VK_LBUTTON.0.into()) } < 0;
      if !is_pressed {
        break;
      }
      std::thread::sleep(Duration::from_millis(8));
    }

    let _ = finish_recording_dock_drag(app);
    DOCK_DRAG_ACTIVE.store(false, Ordering::Relaxed);
  });
}

pub fn hide_recording_dock(app: &AppHandle) -> tauri::Result<()> {
  if let Some(dock) = app.get_webview_window(WindowLabel::RecordingDock.as_str()) {
    platform::hide(&dock)?;
  }

  Ok(())
}

#[cfg(test)]
mod tests;
