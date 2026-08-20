// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Manager};

use crate::windows::{self, WindowLabel};

#[tauri::command]
pub fn update_checks_enabled() -> bool {
  !cfg!(debug_assertions)
}

#[tauri::command]
pub fn show_update_prompt(app: AppHandle) -> tauri::Result<()> {
  let window = app
    .get_webview_window(WindowLabel::Update.as_str())
    .ok_or(tauri::Error::WindowNotFound)?;
  #[cfg(target_os = "macos")]
  app.set_dock_visibility(true)?;
  windows::show(&window, true)?;
  windows::contain_normal_window(&app, &window)
}

#[tauri::command]
pub fn hide_update_prompt(app: AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::Update.as_str()) {
    window.hide()?;
  }
  windows::sync_dock_visibility(&app)
}
