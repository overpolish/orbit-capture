use tauri::image::Image;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";
const OPEN_MENU_ID: &str = "open-orbit-capture";
const QUIT_MENU_ID: &str = "quit-orbit-capture";
const TRAY_ID: &str = "orbit-capture";

pub fn initialize(app: &mut App) -> tauri::Result<()> {
  let menu = MenuBuilder::new(app)
    .text(OPEN_MENU_ID, "Open Orbit Capture")
    .separator()
    .text(QUIT_MENU_ID, "Quit Orbit Capture")
    .build()?;

  #[cfg(target_os = "windows")]
  let icon = Image::from_bytes(include_bytes!("../icons/tray-default.ico"))?;

  #[cfg(not(target_os = "windows"))]
  let icon = Image::from_bytes(include_bytes!("../icons/tray-default.png"))?;

  TrayIconBuilder::with_id(TRAY_ID)
    .icon(icon)
    .icon_as_template(cfg!(target_os = "macos"))
    .menu(&menu)
    .show_menu_on_left_click(false)
    .tooltip("Orbit Capture")
    .on_menu_event(|app, event| match event.id().as_ref() {
      OPEN_MENU_ID => show_main_window(app),
      QUIT_MENU_ID => app.exit(0),
      _ => {}
    })
    .on_tray_icon_event(|tray, event| {
      if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
      } = event
      {
        show_main_window(tray.app_handle());
      }
    })
    .build(app)?;

  if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window_to_hide.hide();
      }
    });
  }

  Ok(())
}

fn show_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
  }
}
