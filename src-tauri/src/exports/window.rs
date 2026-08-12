// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Manager};

use crate::windows::{self, WindowLabel};

pub fn show(app: &AppHandle) -> tauri::Result<()> {
  let window = app
    .get_webview_window(WindowLabel::Export.as_str())
    .ok_or(tauri::Error::WindowNotFound)?;

  #[cfg(target_os = "macos")]
  app.set_dock_visibility(true)?;

  windows::show(&window, true)?;
  let _ = windows::contain_export(app, &window);

  Ok(())
}

pub fn hide(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::Export.as_str()) {
    window.hide()?;
  }

  windows::sync_dock_visibility(app)?;

  Ok(())
}
