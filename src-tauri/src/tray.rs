// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::image::Image;
use tauri::menu::{Menu, MenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Wry};

use crate::recording::RecordingStatus;
use crate::windows;

const DISCARD_MENU_ID: &str = "discard-recording";
const OPEN_MENU_ID: &str = "open-orbit-capture";
const PAUSE_MENU_ID: &str = "pause-recording";
const QUIT_MENU_ID: &str = "quit-orbit-capture";
const STOP_MENU_ID: &str = "stop-recording";
const TRAY_ID: &str = "orbit-capture";

#[cfg(target_os = "windows")]
fn status_icon(status: RecordingStatus) -> tauri::Result<Image<'static>> {
  Image::from_bytes(match status {
    RecordingStatus::Idle => include_bytes!("../icons/tray-default.ico").as_slice(),
    RecordingStatus::Starting | RecordingStatus::Stopping => {
      include_bytes!("../icons/tray-loading.ico").as_slice()
    }
    RecordingStatus::Recording => include_bytes!("../icons/tray-recording.ico").as_slice(),
    RecordingStatus::Paused => include_bytes!("../icons/tray-paused.ico").as_slice(),
  })
}

#[cfg(not(target_os = "windows"))]
fn status_icon(status: RecordingStatus) -> tauri::Result<Image<'static>> {
  Image::from_bytes(match status {
    RecordingStatus::Idle => include_bytes!("../icons/tray-default.png").as_slice(),
    RecordingStatus::Starting | RecordingStatus::Stopping => {
      include_bytes!("../icons/tray-loading.png").as_slice()
    }
    RecordingStatus::Recording => include_bytes!("../icons/tray-recording.png").as_slice(),
    RecordingStatus::Paused => include_bytes!("../icons/tray-paused.png").as_slice(),
  })
}

const fn status_tooltip(status: RecordingStatus) -> &'static str {
  match status {
    RecordingStatus::Idle => "Orbit Capture",
    RecordingStatus::Starting => "Orbit Capture - Starting a recording",
    RecordingStatus::Recording => "Orbit Capture - Recording",
    RecordingStatus::Paused => "Orbit Capture - Recording paused",
    RecordingStatus::Stopping => "Orbit Capture - Finishing the recording",
  }
}

/// The recording controls join the menu only while there is a recording to
/// control. Quit always stays, because the tray is not the only way out.
fn build_menu(app: &AppHandle, status: RecordingStatus) -> tauri::Result<Menu<Wry>> {
  let mut builder = MenuBuilder::new(app).text(OPEN_MENU_ID, "Open Orbit Capture");

  if matches!(status, RecordingStatus::Recording | RecordingStatus::Paused) {
    let pause_label = if status == RecordingStatus::Paused {
      "Resume Recording"
    } else {
      "Pause Recording"
    };
    builder = builder
      .separator()
      .text(PAUSE_MENU_ID, pause_label)
      .text(STOP_MENU_ID, "Stop Recording")
      .text(DISCARD_MENU_ID, "Discard Recording");
  }

  builder
    .separator()
    .text(QUIT_MENU_ID, "Quit Orbit Capture")
    .build()
}

pub fn initialize(app: &mut App) -> tauri::Result<()> {
  let menu = build_menu(app.handle(), RecordingStatus::Idle)?;

  TrayIconBuilder::with_id(TRAY_ID)
    .icon(status_icon(RecordingStatus::Idle)?)
    .icon_as_template(cfg!(target_os = "macos"))
    .menu(&menu)
    .show_menu_on_left_click(false)
    .tooltip(status_tooltip(RecordingStatus::Idle))
    .on_menu_event(|app, event| match event.id().as_ref() {
      DISCARD_MENU_ID => report("discard", crate::recording::cancel(app)),
      OPEN_MENU_ID => show_main_window(app),
      PAUSE_MENU_ID => report("pause", crate::recording::toggle_pause(app)),
      QUIT_MENU_ID => app.exit(0),
      STOP_MENU_ID => report("stop", crate::recording::stop(app)),
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

/// Reflects the recording state in the tray. Every piece of state is read
/// lazily inside the handlers, so nothing here depends on setup ordering.
pub fn apply_recording_status(app: &AppHandle, status: RecordingStatus) {
  let app = app.clone();
  // Transitions are driven from command threads and background tasks, while
  // menus and tray icons are main-thread objects.
  let _ = app.clone().run_on_main_thread(move || {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
      return;
    };

    if let Ok(icon) = status_icon(status) {
      let _ = tray.set_icon(Some(icon));
    }

    #[cfg(target_os = "macos")]
    let _ = tray.set_icon_as_template(true);

    let _ = tray.set_tooltip(Some(status_tooltip(status)));

    if let Ok(menu) = build_menu(&app, status) {
      let _ = tray.set_menu(Some(menu));
    }
  });
}

fn report(action: &str, result: Result<(), String>) {
  if let Err(error) = result {
    eprintln!("Could not {action} the recording from the tray: {error}");
  }
}

fn show_main_window(app: &AppHandle) {
  #[cfg(target_os = "macos")]
  if !crate::permissions::has_required_recording_permissions(app) {
    let _ = crate::permissions::show_permissions_window(app);
    return;
  }

  let _ = windows::show_recording_ui(app);
}
