// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::ipc::{Channel, InvokeResponseBody};

#[tauri::command]
pub async fn take_monitor_screenshot(monitor_id: u32, channel: Channel) -> Result<(), String> {
  let screenshot = tauri::async_runtime::spawn_blocking(move || {
    let monitor = xcap::Monitor::all()
      .map_err(|error| error.to_string())?
      .into_iter()
      .find(|monitor| monitor.id().ok() == Some(monitor_id))
      .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
    monitor
      .capture_image()
      .map(|image| image.into_raw())
      .map_err(|error| error.to_string())
  })
  .await
  .map_err(|error| error.to_string())??;

  channel
    .send(InvokeResponseBody::Raw(screenshot))
    .map_err(|error| error.to_string())
}
