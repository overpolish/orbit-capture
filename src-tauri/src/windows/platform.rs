// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
use tauri_nspanel::{
  tauri_panel, CollectionBehavior, ManagerExt as PanelManagerExt, PanelHandle, PanelLevel,
  StyleMask, TrackingAreaOptions, WebviewWindowExt,
};

#[cfg(target_os = "macos")]
tauri_panel! {
  panel!(RecordingBarPanel {
    config: {
      can_become_key_window: true,
      can_become_main_window: false,
      becomes_key_only_if_needed: true,
      hides_on_deactivate: false,
      is_floating_panel: true,
      works_when_modal: true
    }
    with: {
      tracking_area: {
        options: TrackingAreaOptions::new()
          .active_always()
          .mouse_entered_and_exited()
          .mouse_moved()
          .cursor_update(),
        auto_resize: true
      }
    }
  })
}

#[cfg(target_os = "macos")]
fn configure_panel<T: tauri_nspanel::FromWindow<tauri::Wry> + 'static>(
  window: &WebviewWindow,
  level: i32,
) -> tauri::Result<()> {
  let panel = window.to_panel::<T>()?;

  panel.set_level(PanelLevel::Custom(level).value());
  panel.set_style_mask(StyleMask::empty().nonactivating_panel().into());
  panel.set_collection_behavior(
    CollectionBehavior::new()
      .full_screen_auxiliary()
      .can_join_all_spaces()
      .transient()
      .into(),
  );
  panel.set_hides_on_deactivate(false);
  panel.set_works_when_modal(true);
  panel.set_accepts_mouse_moved_events(true);
  window.hide()?;

  Ok(())
}

#[cfg(target_os = "macos")]
fn registered_panel(window: &WebviewWindow) -> tauri::Result<PanelHandle<tauri::Wry>> {
  window
    .app_handle()
    .get_webview_panel(window.label())
    .map_err(|_| tauri::Error::WindowNotFound)
}

#[cfg(target_os = "macos")]
pub fn initialize_recording_bar(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 28)
}

#[cfg(target_os = "macos")]
pub fn initialize_recording_source_selector(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 29)
}

#[cfg(target_os = "macos")]
pub fn initialize_region_selector(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 27)
}

#[cfg(target_os = "macos")]
pub fn initialize_recording_options(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 30)
}

#[cfg(target_os = "macos")]
pub fn initialize_standalone_listbox(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 31)
}

// The pill sits above every other panel so it stays reachable over the
// click-through region overlay and over fullscreen apps.
#[cfg(target_os = "macos")]
pub fn initialize_recording_dock(window: &WebviewWindow) -> tauri::Result<()> {
  configure_panel::<RecordingBarPanel>(window, 32)
}

/// The export window is an ordinary focusable window, so it gets none of the
/// panel treatment - only the capture exclusion, so that taking a screenshot
/// while it is open never pictures it. On macOS every window this process owns
/// is already excluded by owning-process, so there is nothing to do.
#[cfg(target_os = "macos")]
pub fn initialize_export(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

/// Above every panel this app owns, which run 27 to 32.
#[cfg(target_os = "macos")]
const EXPORT_WINDOW_LEVEL: isize = 33;

/// Lifts the export window over our own overlays.
///
/// `always_on_top` only puts an ordinary window at the floating level, which is
/// below all of our panels, so a region capture would open the window beneath
/// its own overlay. A window level is independent of key/focus state, so this
/// does not stop the file name field from taking input.
#[cfg(target_os = "macos")]
pub fn raise_export(window: &WebviewWindow) -> tauri::Result<()> {
  use objc2_app_kit::NSWindow;

  let address = window.ns_window()? as usize;
  window.app_handle().run_on_main_thread(move || {
    // SAFETY: Tauri hands back a live NSWindow for a macOS webview window, and
    // this closure runs on the thread that owns it.
    let ns_window: &NSWindow = unsafe { &*(address as *const NSWindow) };
    ns_window.setLevel(EXPORT_WINDOW_LEVEL);
  })
}

#[cfg(target_os = "macos")]
pub fn set_opacity(window: &WebviewWindow, opacity: f64) -> tauri::Result<()> {
  let panel = registered_panel(window)?;
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || panel.set_alpha_value(opacity))
}

#[cfg(target_os = "macos")]
pub fn resign_key(window: &WebviewWindow) -> tauri::Result<()> {
  let panel = registered_panel(window)?;
  panel.resign_key_window();
  Ok(())
}

#[cfg(target_os = "macos")]
pub fn restore_recording_level(window: &WebviewWindow) -> tauri::Result<()> {
  let level = match window.label() {
    "region-selector" => 27,
    "recording-bar" => 28,
    "recording-source-selector" => 29,
    "recording-options" => 30,
    "standalone-listbox" => 31,
    "recording-dock" => 32,
    _ => return Ok(()),
  };
  let panel = registered_panel(window)?;
  panel.set_level(PanelLevel::Custom(level).value());
  Ok(())
}

#[cfg(target_os = "macos")]
pub fn raise_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
  registered_panel(window)?.show();
  restore_recording_level(window)
}

#[cfg(target_os = "macos")]
pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_ignore_cursor_events(true)?;
  let panel = registered_panel(window)?;
  let window = window.clone();
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || {
    panel.set_alpha_value(0.0);
    let _ = window.hide();
    panel.hide();
  })
}

#[cfg(target_os = "macos")]
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_ignore_cursor_events(false)?;
  let panel = registered_panel(window)?;
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || {
    panel.set_alpha_value(1.0);
    panel.show();
  })
}

/// Every window this app floats over the desktop is an overlay: always on top,
/// off the taskbar, and excluded from capture so it can never appear in a
/// screenshot or recording - including the ones this app takes itself. That
/// exclusion is what removes the need to hide the UI before capturing.
#[cfg(target_os = "windows")]
fn initialize_overlay(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_always_on_top(true)?;
  window.set_skip_taskbar(true)?;
  exclude_from_capture(window)
}

#[cfg(target_os = "windows")]
fn exclude_from_capture(window: &WebviewWindow) -> tauri::Result<()> {
  use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
  };

  unsafe {
    SetWindowDisplayAffinity(HWND(window.hwnd()?.0), WDA_EXCLUDEFROMCAPTURE)
      .map_err(std::io::Error::other)?;
  }

  Ok(())
}

#[cfg(target_os = "windows")]
pub fn initialize_recording_bar(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_recording_source_selector(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_region_selector(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_recording_options(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_standalone_listbox(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_recording_dock(window: &WebviewWindow) -> tauri::Result<()> {
  initialize_overlay(window)
}

#[cfg(target_os = "windows")]
pub fn initialize_export(window: &WebviewWindow) -> tauri::Result<()> {
  exclude_from_capture(window)
}

/// The overlays are all topmost, so re-asserting z-order on show puts the
/// export window at the front of that band without taking focus off it.
#[cfg(target_os = "windows")]
pub fn raise_export(window: &WebviewWindow) -> tauri::Result<()> {
  raise_without_activation(window)
}

#[cfg(target_os = "windows")]
pub fn restore_recording_level(window: &WebviewWindow) -> tauri::Result<()> {
  raise_without_activation(window)
}

#[cfg(target_os = "windows")]
pub fn set_opacity(window: &WebviewWindow, opacity: f64) -> tauri::Result<()> {
  use windows::Win32::{
    Foundation::{COLORREF, HWND},
    UI::WindowsAndMessaging::{
      GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA,
      WS_EX_LAYERED,
    },
  };

  let hwnd = HWND(window.hwnd()?.0);
  unsafe {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as isize);
    SetLayeredWindowAttributes(
      hwnd,
      COLORREF(0),
      (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
      LWA_ALPHA,
    )
    .map_err(std::io::Error::other)?;
  }
  Ok(())
}

#[cfg(target_os = "windows")]
pub fn raise_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
  use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE},
  };

  unsafe {
    SetWindowPos(
      HWND(window.hwnd()?.0),
      Some(HWND_TOPMOST),
      0,
      0,
      0,
      0,
      SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    )
    .map_err(std::io::Error::other)?;
  }
  Ok(())
}

#[cfg(target_os = "windows")]
pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
  window.hide()
}

#[cfg(target_os = "windows")]
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.show()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_recording_bar(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_recording_source_selector(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_region_selector(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_recording_options(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_standalone_listbox(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_recording_dock(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn initialize_export(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn raise_export(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
  window.hide()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.show()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_opacity(_window: &WebviewWindow, _opacity: f64) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn restore_recording_level(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn raise_without_activation(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}
