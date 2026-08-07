use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent};

#[derive(Clone, Copy)]
pub enum WindowLabel {
  Main,
  Permissions,
}

impl WindowLabel {
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Main => "main",
      Self::Permissions => "permissions",
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

pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.show()?;
  window.unminimize()?;
  window.set_focus()
}

pub fn show_existing(app: &AppHandle, label: WindowLabel) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(label.as_str()) {
    show(&window)?;
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
