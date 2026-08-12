// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

pub(crate) mod preferences;

pub use preferences::{current, initialize, GeneralSettingsState};

use tauri::{AppHandle, Manager};

use crate::windows::{self, WindowLabel};

pub fn show(app: &AppHandle) -> tauri::Result<()> {
  let window = app
    .get_webview_window(WindowLabel::Settings.as_str())
    .ok_or(tauri::Error::WindowNotFound)?;
  #[cfg(target_os = "macos")]
  app.set_dock_visibility(true)?;
  windows::show(&window, true)?;
  windows::contain_normal_window(app, &window)
}

#[tauri::command]
pub fn hide_settings(app: AppHandle) -> tauri::Result<()> {
  let _ = crate::shortcuts::end_shortcut_capture(app.clone());
  if let Some(window) = app.get_webview_window(WindowLabel::Settings.as_str()) {
    window.hide()?;
  }
  windows::sync_dock_visibility(&app)
}

#[tauri::command]
pub fn show_settings(app: AppHandle) -> tauri::Result<()> {
  show(&app)
}
