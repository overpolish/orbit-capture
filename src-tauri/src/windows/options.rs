// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};

use super::{platform, WindowLabel};

const RECORDING_OPTIONS_WIDTH: f64 = 240.0;
const RECORDING_OPTIONS_HEIGHT: f64 = 300.0;
const RECORDING_OPTIONS_GAP: f64 = 6.0;
static RECORDING_OPTIONS_VISIBLE: AtomicBool = AtomicBool::new(false);
static STANDALONE_LISTBOX_VISIBLE: AtomicBool = AtomicBool::new(false);

fn recording_options_frame(app: &AppHandle, anchor_x: f64) -> tauri::Result<LogicalPosition<f64>> {
  let bar = app
    .get_webview_window(WindowLabel::RecordingBar.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let monitor = bar
    .current_monitor()?
    .or(app.primary_monitor()?)
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let scale = monitor.scale_factor();
  let monitor_position = monitor.position().to_logical::<f64>(scale);
  let monitor_size = monitor.size().to_logical::<f64>(scale);
  let bar_position = bar.outer_position()?.to_logical::<f64>(scale);
  let bar_size = bar.outer_size()?.to_logical::<f64>(scale);
  let monitor_right = monitor_position.x + monitor_size.width;
  let monitor_bottom = monitor_position.y + monitor_size.height;
  let x = (bar_position.x + anchor_x - RECORDING_OPTIONS_WIDTH / 2.0)
    .clamp(monitor_position.x, monitor_right - RECORDING_OPTIONS_WIDTH);
  let available_above = bar_position.y - monitor_position.y;
  let y = if available_above >= RECORDING_OPTIONS_HEIGHT + RECORDING_OPTIONS_GAP {
    bar_position.y - RECORDING_OPTIONS_HEIGHT - RECORDING_OPTIONS_GAP
  } else {
    (bar_position.y + bar_size.height + RECORDING_OPTIONS_GAP)
      .min(monitor_bottom - RECORDING_OPTIONS_HEIGHT)
  };

  Ok(LogicalPosition::new(x, y))
}

#[tauri::command]
pub fn toggle_recording_options(app: AppHandle, anchor_x: f64) -> tauri::Result<()> {
  if !crate::recording::is_idle(&app) {
    return Ok(());
  }

  if RECORDING_OPTIONS_VISIBLE.load(Ordering::Relaxed) {
    return hide_recording_options(app);
  }

  let window = app
    .get_webview_window(WindowLabel::RecordingOptions.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  window.set_size(LogicalSize::new(
    RECORDING_OPTIONS_WIDTH,
    RECORDING_OPTIONS_HEIGHT,
  ))?;
  window.set_position(recording_options_frame(&app, anchor_x)?)?;
  RECORDING_OPTIONS_VISIBLE.store(true, Ordering::Relaxed);
  platform::show(&window)?;
  platform::restore_recording_level(&window)?;
  app.emit_to(
    WindowLabel::RecordingOptions.as_str(),
    "recording-options://opened",
    (),
  )
}

#[tauri::command]
pub fn hide_recording_options(app: AppHandle) -> tauri::Result<()> {
  RECORDING_OPTIONS_VISIBLE.store(false, Ordering::Relaxed);
  hide_standalone_listbox(app.clone())?;
  crate::audio_preview::stop_all(&app);
  crate::camera_preview::stop_all(&app);
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingOptions.as_str()) {
    platform::hide(&window)?;
  }
  app.emit_to(
    WindowLabel::RecordingOptions.as_str(),
    "recording-options://closed",
    (),
  )
}

#[tauri::command]
pub fn show_standalone_listbox(
  app: AppHandle,
  parent_window_label: String,
  offset: LogicalPosition<f64>,
  size: LogicalSize<f64>,
) -> tauri::Result<()> {
  let parent = app
    .get_webview_window(&parent_window_label)
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let window = app
    .get_webview_window(WindowLabel::StandaloneListbox.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  let scale = parent.scale_factor()?;
  let parent_position = parent.outer_position()?.to_logical::<f64>(scale);
  let mut position =
    LogicalPosition::new(parent_position.x + offset.x, parent_position.y + offset.y);

  if let Some(monitor) = parent.current_monitor()?.or(app.primary_monitor()?) {
    let monitor_scale = monitor.scale_factor();
    let monitor_position = monitor.position().to_logical::<f64>(monitor_scale);
    let monitor_size = monitor.size().to_logical::<f64>(monitor_scale);
    let max_x = monitor_position.x + (monitor_size.width - size.width).max(0.0);
    let max_y = monitor_position.y + (monitor_size.height - size.height).max(0.0);
    position.x = position.x.clamp(monitor_position.x, max_x);
    position.y = position.y.clamp(monitor_position.y, max_y);
  }

  window.set_size(size)?;
  window.set_position(position)?;
  STANDALONE_LISTBOX_VISIBLE.store(true, Ordering::Relaxed);
  platform::show(&window)?;
  platform::restore_recording_level(&window)
}

#[tauri::command]
pub fn hide_standalone_listbox(app: AppHandle) -> tauri::Result<()> {
  STANDALONE_LISTBOX_VISIBLE.store(false, Ordering::Relaxed);
  if let Some(window) = app.get_webview_window(WindowLabel::StandaloneListbox.as_str()) {
    platform::hide(&window)?;
  }
  app.emit("standalone-listbox://closed", ())?;

  Ok(())
}

pub(super) fn coordinate_is_in_window(x: f64, y: f64, window: &WebviewWindow) -> bool {
  let Ok(position) = window.outer_position() else {
    return false;
  };
  let Ok(size) = window.outer_size() else {
    return false;
  };
  let Ok(scale) = window.scale_factor() else {
    return false;
  };
  let position = position.to_logical::<f64>(scale);
  let size = size.to_logical::<f64>(scale);

  x >= position.x
    && x <= position.x + size.width
    && y >= position.y
    && y <= position.y + size.height
}

pub(super) fn dismiss_if_outside(app: &AppHandle, x: f64, y: f64) {
  if !RECORDING_OPTIONS_VISIBLE.load(Ordering::Relaxed) {
    return;
  }

  let is_in = |label: WindowLabel| {
    app
      .get_webview_window(label.as_str())
      .is_some_and(|window| coordinate_is_in_window(x, y, &window))
  };
  let inside_options = is_in(WindowLabel::RecordingOptions);
  let inside_listbox =
    STANDALONE_LISTBOX_VISIBLE.load(Ordering::Relaxed) && is_in(WindowLabel::StandaloneListbox);
  let inside_bar = is_in(WindowLabel::RecordingBar);

  if !inside_options && !inside_listbox && !inside_bar {
    let _ = hide_recording_options(app.clone());
  }
}
