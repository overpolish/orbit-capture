use tauri::image::Image;
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle};

use crate::windows;
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

  Ok(())
}

fn show_main_window(app: &AppHandle) {
  #[cfg(target_os = "macos")]
  if !crate::permissions::has_required_recording_permissions(app) {
    let _ = crate::permissions::show_permissions_window(app);
    return;
  }

  let _ = windows::show_recording_ui(app);
}
