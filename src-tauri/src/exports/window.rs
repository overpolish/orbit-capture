// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Manager};

use super::ExportKind;
use crate::windows;

pub fn show(app: &AppHandle, kind: ExportKind) -> tauri::Result<()> {
  let window = app
    .get_webview_window(kind.window_label().as_str())
    .ok_or(tauri::Error::WindowNotFound)?;

  #[cfg(target_os = "macos")]
  app.set_dock_visibility(true)?;

  windows::show(&window, true)?;
  let _ = windows::contain_export(app, &window);

  Ok(())
}

pub fn hide(app: &AppHandle, kind: ExportKind) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(kind.window_label().as_str()) {
    window.hide()?;
  }

  windows::sync_dock_visibility(app)?;

  Ok(())
}
