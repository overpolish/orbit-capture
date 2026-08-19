// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{
  ipc::{Channel, InvokeResponseBody},
  AppHandle,
};

/// The monitor image behind the region overlay, for its magnifier.
///
/// Screenwide's own windows are left out so the overlay can stay on screen
/// while this is taken: macOS excludes them per capture, and on Windows they
/// carry the exclude-from-capture affinity for the split second of the shot
/// even when "Record Screenwide's windows" would otherwise keep them in.
#[tauri::command]
pub async fn take_monitor_screenshot(
  app: AppHandle,
  monitor_id: u32,
  channel: Channel,
) -> Result<(), String> {
  #[cfg(not(target_os = "windows"))]
  let _ = &app;
  #[cfg(target_os = "windows")]
  let restore_affinity = crate::settings::current(&app).record_screenwide_windows;
  #[cfg(target_os = "windows")]
  if restore_affinity {
    crate::windows::sync_capture_affinity(&app, false).map_err(|error| error.to_string())?;
  }

  let screenshot = tauri::async_runtime::spawn_blocking(move || {
    #[cfg(target_os = "macos")]
    {
      crate::screenshots::capture_monitor_without_own_windows_blocking(monitor_id)
    }
    #[cfg(not(target_os = "macos"))]
    {
      let monitor = xcap::Monitor::all()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|monitor| monitor.id().ok() == Some(monitor_id))
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      monitor
        .capture_image()
        .map(|image| image.into_raw())
        .map_err(|error| error.to_string())
    }
  })
  .await
  .map_err(|error| error.to_string());

  // Put the affinity back before the result is looked at, so a failed capture
  // cannot leave the windows excluded from the user's recordings.
  #[cfg(target_os = "windows")]
  if restore_affinity {
    crate::windows::sync_capture_affinity(&app, true).map_err(|error| error.to_string())?;
  }
  let screenshot = screenshot??;

  channel
    .send(InvokeResponseBody::Raw(screenshot))
    .map_err(|error| error.to_string())
}
