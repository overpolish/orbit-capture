// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::{capture_overlays, screenshots};

pub(crate) mod analysis;
pub(crate) mod focus;
pub(crate) mod snapshot;
use focus::{follow_cursor_focus, watch_focus, FocusRegion};
pub use snapshot::RulerState;

const WINDOW_PREFIX: &str = "ruler-";
const SCREENSHOT_MODE_EVENT: &str = "ruler://screenshot-mode";

/// The region editor takes focus above a deliberately preserved ruler, so the
/// blur it causes must not tear the session down.
static SCREENSHOT_MODE: AtomicBool = AtomicBool::new(false);

fn set_ruler_level(window: &tauri::WebviewWindow, level: isize) -> Result<(), String> {
  #[cfg(target_os = "macos")]
  {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let window = window.clone();
    let app = window.app_handle().clone();
    app
      .run_on_main_thread(move || {
        let result = window
          .ns_window()
          .map_err(|error| error.to_string())
          .map(|raw_window| {
            let native_window: &objc2_app_kit::NSWindow = unsafe { &*raw_window.cast() };
            native_window.setLevel(level);
          });
        let _ = sender.send(result);
      })
      .map_err(|error| error.to_string())?;
    receiver.recv().map_err(|error| error.to_string())?
  }

  #[cfg(not(target_os = "macos"))]
  {
    let _ = window;
    Ok(())
  }
}

fn ruler_windows(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
  capture_overlays::windows(app, WINDOW_PREFIX)
}

fn close_ruler_windows(app: &AppHandle) {
  capture_overlays::close_windows(app, WINDOW_PREFIX, None);
}

pub fn dismiss(app: &AppHandle) {
  SCREENSHOT_MODE.store(false, Ordering::Relaxed);
  let had_windows = !ruler_windows(app).is_empty();
  close_ruler_windows(app);
  let had_capture = app.state::<RulerState>().cancel();
  if had_windows || had_capture {
    capture_overlays::emit_lifecycle(app, false);
  }
}

pub fn is_active(app: &AppHandle) -> bool {
  !ruler_windows(app).is_empty()
}

#[tauri::command]
pub fn set_ruler_screenshot_mode(app: AppHandle, active: bool) -> Result<(), String> {
  SCREENSHOT_MODE.store(active, Ordering::Relaxed);
  for window in ruler_windows(&app) {
    // The region editor must sit above the ruler while the shot is framed;
    // the ruler remains visible underneath and returns to its normal level.
    set_ruler_level(&window, if active { 26 } else { 33 })?;
    window
      .emit(SCREENSHOT_MODE_EVENT, active)
      .map_err(|error| error.to_string())?;
    window
      .set_ignore_cursor_events(active)
      .map_err(|error| error.to_string())?;
  }
  Ok(())
}

pub async fn start(app: &AppHandle) -> Result<(), String> {
  dismiss(app);
  capture_overlays::dismiss_except(app, Some(capture_overlays::CaptureOverlay::Ruler));
  let generation = app.state::<RulerState>().begin();

  let monitors = capture_overlays::monitor_layout(app)?;
  let mut snapshots = Vec::with_capacity(monitors.len());
  for (monitor_id, scale, _) in &monitors {
    let image = screenshots::capture_overlay_snapshot(*monitor_id).await?;
    snapshots.push((*monitor_id, *scale, image));
  }
  if !app.state::<RulerState>().install(generation, snapshots) {
    return Ok(());
  }

  let mut regions = Vec::new();
  for (index, (monitor_id, scale, monitor)) in monitors.into_iter().enumerate() {
    let position = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    // Logical coordinates on macOS (mixed-DPI physical rects share no global
    // space); physical elsewhere — matching `focus::poll_cursor`.
    #[cfg(target_os = "macos")]
    regions.push(FocusRegion {
      height: size.height,
      label: format!("{WINDOW_PREFIX}{index}"),
      width: size.width,
      x: position.x,
      y: position.y,
    });
    #[cfg(not(target_os = "macos"))]
    regions.push(FocusRegion {
      height: f64::from(monitor.size().height),
      label: format!("{WINDOW_PREFIX}{index}"),
      width: f64::from(monitor.size().width),
      x: f64::from(monitor.position().x),
      y: f64::from(monitor.position().y),
    });
    let window = WebviewWindowBuilder::new(
      app,
      format!("{WINDOW_PREFIX}{index}"),
      WebviewUrl::App(format!("/ruler?monitorId={monitor_id}").into()),
    )
    .accept_first_mouse(true)
    .always_on_top(true)
    .decorations(false)
    .focused(index == 0)
    .inner_size(size.width, size.height)
    .position(position.x, position.y)
    .resizable(false)
    .shadow(false)
    .skip_taskbar(true)
    .transparent(true)
    .visible(false)
    .visible_on_all_workspaces(true)
    .build()
    .map_err(|error| error.to_string())?;
    // Deliberately shareable: the screenshot shortcut can preserve the ruler
    // and capture its annotations as part of the selected desktop region.
    set_ruler_level(&window, 33)?;
    // Switching to any other app - or any other window of ours - ends the
    // session, exactly as the cancel command does.
    watch_focus(&window);
    crate::windows::show(&window, index == 0).map_err(|error| error.to_string())?;
  }
  follow_cursor_focus(app, regions);

  capture_overlays::emit_lifecycle(app, true);

  Ok(())
}

pub fn start_detached(app: &AppHandle) {
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    if let Err(error) = start(&app).await {
      eprintln!("Could not start ruler overlay: {error}");
    }
  });
}

#[tauri::command]
pub async fn start_ruler(app: AppHandle) -> Result<(), String> {
  start(&app).await
}

#[tauri::command]
pub fn cancel_ruler(app: AppHandle) {
  dismiss(&app);
}

#[tauri::command]
pub fn copy_ruler_value(app: AppHandle, value: String) -> Result<(), String> {
  app
    .clipboard()
    .write_text(value)
    .map_err(|error| error.to_string())
}
