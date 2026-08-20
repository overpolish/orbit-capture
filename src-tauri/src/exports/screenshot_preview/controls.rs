// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::Manager;

use super::state::ScreenshotPreviewState;

#[tauri::command]
pub fn set_screenshot_preview_zoom(
  state: tauri::State<'_, ScreenshotPreviewState>,
  session_id: u64,
  zoom_percent: f64,
) -> Result<(), String> {
  if !zoom_percent.is_finite() || !(10.0..=1_600.0).contains(&zoom_percent) {
    return Err("The screenshot preview zoom is invalid".to_owned());
  }
  let surface = {
    let manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.surface.clone()
  };
  if let Some(surface) = surface {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    surface.set_editor_zoom(zoom_percent);
  }
  Ok(())
}

#[tauri::command]
pub fn center_screenshot_preview_workspace(app: tauri::AppHandle) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || {
    let surface = app
      .state::<ScreenshotPreviewState>()
      .0
      .lock()
      .ok()
      .and_then(|manager| manager.surface.clone());
    if let Some(surface) = surface {
      #[cfg(any(target_os = "macos", target_os = "windows"))]
      surface.center_editor();
    }
  });
  Ok(())
}

#[tauri::command]
pub fn stop_screenshot_preview(
  state: tauri::State<'_, ScreenshotPreviewState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  if manager.session_id == Some(session_id) {
    manager.stop();
  }
  Ok(())
}
