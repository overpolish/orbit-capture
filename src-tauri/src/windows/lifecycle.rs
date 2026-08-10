// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use super::{
  geometry::{contain_window_in_work_area, keep_window_on_a_monitor},
  platform, WindowLabel,
};

const EXPORT_WINDOW_SETTLE_TIME: Duration = Duration::from_millis(150);

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

pub fn initialize_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::initialize_recording_bar(&window)?;
  }

  Ok(())
}

pub fn initialize_recording_source_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
    platform::initialize_recording_source_selector(&window)?;
  }

  Ok(())
}

pub fn initialize_region_selector(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    platform::initialize_region_selector(&window)?;
    window.set_ignore_cursor_events(true)?;
  }

  Ok(())
}

pub fn initialize_recording_options(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingOptions.as_str()) {
    platform::initialize_recording_options(&window)?;
  }

  Ok(())
}

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

  let (movement, movements) = mpsc::channel();
  window.on_window_event(move |event| {
    if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
      let _ = movement.send(());
    }
  });

  let app = window.app_handle().clone();
  let export = window.clone();
  std::thread::spawn(move || {
    while movements.recv().is_ok() {
      loop {
        match movements.recv_timeout(EXPORT_WINDOW_SETTLE_TIME) {
          Ok(()) => {}
          Err(RecvTimeoutError::Timeout) => {
            let _ = contain_window_in_work_area(&app, &export);
            break;
          }
          Err(RecvTimeoutError::Disconnected) => return,
        }
      }
    }
  });

  Ok(())
}

pub fn contain_export(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
  contain_window_in_work_area(app, window)
}
