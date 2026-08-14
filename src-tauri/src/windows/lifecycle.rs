// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use super::{
  geometry::{contain_window_in_work_area, keep_window_on_a_monitor},
  platform, WindowLabel,
};

static EXPORT_DRAG_ACTIVE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
pub fn get_or_create<F>(
  app: &AppHandle,
  label: WindowLabel,
  create: F,
) -> tauri::Result<WebviewWindow>
where
  F: FnOnce() -> tauri::Result<WebviewWindow>,
{
  app
    .get_webview_window(label.as_str())
    .map_or_else(create, Ok)
}

pub fn show(window: &WebviewWindow, focus: bool) -> tauri::Result<()> {
  platform::prepare_to_show(window)?;
  window.show()?;
  window.unminimize()?;
  if focus {
    window.set_focus()?;
  }

  Ok(())
}

pub fn initialize_recording_bar_position(app: &AppHandle) -> tauri::Result<()> {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) else {
    return Ok(());
  };
  let Some(monitor) = window.current_monitor()? else {
    return Ok(());
  };

  let monitor_position = monitor.position();
  let monitor_size = monitor.size();
  let window_size = window.outer_size()?;

  window.set_position(PhysicalPosition {
    x: monitor_position.x + (monitor_size.width.saturating_sub(window_size.width) / 2) as i32,
    y: monitor_position.y + monitor_size.height.saturating_sub(window_size.height + 100) as i32,
  })?;

  // Restoring after the fallback means the first launch has a sensible
  // position while later launches respect where the user moved the bar.
  let _ = window.restore_state(StateFlags::POSITION);
  keep_window_on_a_monitor(app, &window)?;

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::initialize_recording_bar(&window)?;
  }

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_recording_source_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
    platform::initialize_recording_source_selector(&window)?;
  }

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_region_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    platform::initialize_region_selector(&window)?;
    window.set_ignore_cursor_events(true)?;
  }

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_recording_options(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingOptions.as_str()) {
    platform::initialize_recording_options(&window)?;
  }

  Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn initialize_standalone_listbox(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::StandaloneListbox.as_str()) {
    platform::initialize_standalone_listbox(&window)?;
  }

  Ok(())
}

pub fn initialize_export(window: &WebviewWindow) -> tauri::Result<()> {
  platform::initialize_export(window)?;
  // A bundled macOS application can order its ordinary main window onscreen
  // during application activation even when it was configured as invisible.
  // Export only becomes visible when an artifact is presented.
  window.hide()?;

  crate::exports::preview_platform::prewarm(window.clone());

  let app = window.app_handle().clone();
  let export = window.clone();
  window.on_window_event(move |event| {
    if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
      watch_for_export_mouse_up(app.clone(), export.clone());
    }
  });

  Ok(())
}

pub fn initialize_normal_window(window: &WebviewWindow) -> tauri::Result<()> {
  platform::initialize_export(window)?;
  window.hide()
}

#[cfg(target_os = "macos")]
fn watch_for_export_mouse_up(app: AppHandle, export: WebviewWindow) {
  use cidre::cg::{EventSrcState, MouseButton};

  if EXPORT_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
    return;
  }
  tauri::async_runtime::spawn_blocking(move || {
    while EventSrcState::CombinedSession.button_state(MouseButton::Left) {
      std::thread::sleep(Duration::from_millis(8));
    }
    let _ = contain_window_in_work_area(&app, &export);
    EXPORT_DRAG_ACTIVE.store(false, Ordering::Relaxed);
  });
}

#[cfg(target_os = "windows")]
fn watch_for_export_mouse_up(app: AppHandle, export: WebviewWindow) {
  use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};

  if EXPORT_DRAG_ACTIVE.swap(true, Ordering::Relaxed) {
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
    let _ = contain_window_in_work_area(&app, &export);
    EXPORT_DRAG_ACTIVE.store(false, Ordering::Relaxed);
  });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn watch_for_export_mouse_up(app: AppHandle, export: WebviewWindow) {
  let _ = contain_window_in_work_area(&app, &export);
}

pub fn contain_export(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
  contain_window_in_work_area(app, window)
}

pub fn contain_normal_window(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
  contain_window_in_work_area(app, window)
}

pub fn sync_dock_visibility(_app: &AppHandle) -> tauri::Result<()> {
  #[cfg(target_os = "macos")]
  {
    let visible = [WindowLabel::Export, WindowLabel::Settings]
      .iter()
      .filter_map(|label| _app.get_webview_window(label.as_str()))
      .any(|window| window.is_visible().unwrap_or(false));
    _app.set_dock_visibility(visible)?;
  }

  Ok(())
}
