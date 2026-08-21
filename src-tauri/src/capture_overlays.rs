// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Emitter, Manager};

use crate::windows::WindowLabel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOverlay {
  Ruler,
  TextRecognition,
}

/// Extension point for capture tools that must be mutually exclusive while
/// still allowing one overlay to survive a handoff such as ruler screenshots.
pub fn dismiss_except(app: &AppHandle, preserved: Option<CaptureOverlay>) {
  if preserved != Some(CaptureOverlay::TextRecognition) {
    crate::text_recognition::dismiss(app);
  }
  if preserved != Some(CaptureOverlay::Ruler) {
    crate::ruler::dismiss(app);
  }
}

pub fn dismiss_all(app: &AppHandle) {
  dismiss_except(app, None);
}

pub fn windows(app: &AppHandle, prefix: &str) -> Vec<tauri::WebviewWindow> {
  app
    .webview_windows()
    .into_values()
    .filter(|window| window.label().starts_with(prefix))
    .collect()
}

pub fn close_windows(app: &AppHandle, prefix: &str, except: Option<&str>) {
  for window in windows(app, prefix) {
    if Some(window.label()) != except {
      #[cfg(target_os = "windows")]
      let _ = crate::windows::conceal_disposable_overlay(&window);
      let _ = window.close();
    }
  }
}

pub fn emit_lifecycle(app: &AppHandle, active: bool) {
  let event = if active {
    "capture-overlay://started"
  } else {
    "capture-overlay://ended"
  };
  let _ = app.emit_to(WindowLabel::RecordingBar.as_str(), event, ());
}

// xcap::Monitor wraps a raw display handle that is not Send on every
// platform, so the command future may never hold one across an await.
// Enumerating synchronously drops the handles before the first snapshot.
pub fn monitor_layout(app: &AppHandle) -> Result<Vec<(u32, f64, tauri::Monitor)>, String> {
  let capture_monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
  let tauri_monitors = app
    .available_monitors()
    .map_err(|error| error.to_string())?;
  if capture_monitors.len() != tauri_monitors.len() {
    return Err("Tauri and xcap returned different monitor counts".to_owned());
  }

  capture_monitors
    .into_iter()
    .zip(tauri_monitors)
    .map(|(capture_monitor, monitor)| {
      let monitor_id = capture_monitor.id().map_err(|error| error.to_string())?;
      Ok((monitor_id, monitor.scale_factor(), monitor))
    })
    .collect()
}
