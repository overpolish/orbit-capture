use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};

mod platform;

#[derive(Clone, Copy)]
pub enum WindowLabel {
  Permissions,
  RecordingBar,
}

impl WindowLabel {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Permissions => "permissions",
      Self::RecordingBar => "recording-bar",
    }
  }
}

pub fn get_or_create<F>(
  app: &AppHandle,
  label: WindowLabel,
  create: F,
) -> tauri::Result<WebviewWindow>
where
  F: FnOnce() -> tauri::Result<WebviewWindow>,
{
  app
    .get_webview_window(label.as_str())
    .map_or_else(create, Ok)
}

pub fn show(window: &WebviewWindow, focus: bool) -> tauri::Result<()> {
  window.show()?;
  window.unminimize()?;
  if focus {
    window.set_focus()?;
  }

  Ok(())
}

pub fn show_existing(app: &AppHandle, label: WindowLabel, focus: bool) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(label.as_str()) {
    show(&window, focus)?;
  }

  Ok(())
}

pub fn initialize_recording_bar_position(app: &AppHandle) -> tauri::Result<()> {
  let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) else {
    return Ok(());
  };
  let Some(monitor) = window.current_monitor()? else {
    return Ok(());
  };

  let scale_factor = monitor.scale_factor();
  let monitor_position = monitor.position().to_logical::<f64>(scale_factor);
  let monitor_size = monitor.size().to_logical::<f64>(scale_factor);
  let window_size = window.outer_size()?.to_logical::<f64>(scale_factor);

  window.set_position(LogicalPosition {
    x: monitor_position.x + (monitor_size.width - window_size.width) / 2.0,
    y: monitor_position.y + monitor_size.height - window_size.height - 100.0,
  })?;

  // Restoring after the fallback means the first launch has a sensible
  // position while later launches respect where the user moved the bar.
  let _ = window.restore_state(StateFlags::POSITION);

  Ok(())
}

pub fn initialize_recording_bar(app: &AppHandle) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::initialize_recording_bar(&window)?;
  }

  Ok(())
}

pub fn hide_instead_of_close(app: &AppHandle, label: WindowLabel) {
  if let Some(window) = app.get_webview_window(label.as_str()) {
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window_to_hide.hide();
      }
    });
  }
}
