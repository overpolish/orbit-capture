// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::WebviewWindow;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri::Manager;

#[cfg(target_os = "macos")]
use tauri_nspanel::{
  tauri_panel, CollectionBehavior, ManagerExt as PanelManagerExt, PanelHandle, PanelLevel,
  StyleMask, TrackingAreaOptions, WebviewWindowExt,
};

#[cfg(target_os = "macos")]
tauri_panel! {
  // The recording bar, the source selector, the standalone listbox and the
  // region selector all take typed input at some point, so they have to be
  // able to become key - `becomes_key_only_if_needed` keeps that to the moments
  // a text field actually asks for it.
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

  // The dock is buttons only: it never needs the keyboard, so it refuses key
  // status outright. That way nothing it does can pull keyboard focus off the
  // app the user is recording.
  panel!(RecordingDockPanel {
    config: {
      can_become_key_window: false,
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

  // A hidden Tauri window can still contribute one stale compositor frame as
  // it becomes an NSPanel. Start transparent before AppKit can order it; the
  // explicit show path restores alpha and input together.
  panel.set_alpha_value(0.0);
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
  panel.hide();

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
fn recording_panel_level(window: &WebviewWindow) -> Option<i32> {
  match window.label() {
    "region-selector" => Some(27),
    "recording-bar" => Some(28),
    "recording-source-selector" => Some(29),
    "recording-options" => Some(30),
    "standalone-listbox" => Some(31),
    "recording-dock" => Some(32),
    _ => None,
  }
}

#[cfg(target_os = "macos")]
fn ensure_recording_panel(window: &WebviewWindow) -> tauri::Result<PanelHandle<tauri::Wry>> {
  if let Ok(panel) = registered_panel(window) {
    return Ok(panel);
  }

  let level = recording_panel_level(window).ok_or(tauri::Error::WindowNotFound)?;
  if window.label() == "recording-dock" {
    configure_panel::<RecordingDockPanel>(window, level)?;
  } else {
    configure_panel::<RecordingBarPanel>(window, level)?;
  }
  registered_panel(window)
}

/// The export window is an ordinary focusable window, so it gets none of the
/// panel treatment - only the capture exclusion, so that taking a screenshot
/// while it is open never pictures it. On macOS every window this process owns
/// is already excluded by owning-process, so there is nothing to do.
#[cfg(target_os = "macos")]
pub fn initialize_export(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(target_os = "macos")]
pub fn set_opacity(window: &WebviewWindow, opacity: f64) -> tauri::Result<()> {
  let panel = ensure_recording_panel(window)?;
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || panel.set_alpha_value(opacity))
}

/// Hands keyboard focus back to whatever app owned it before this panel took
/// key status, without hiding the overlay.
///
/// `resignKeyWindow` is a notification AppKit sends itself; calling it directly
/// tells the window it lost focus while WindowServer still routes keystrokes
/// here. For a non-activating panel the only public way to actually give focus
/// up is to leave the window list and come back: ordering the key panel out
/// makes AppKit pick a new key window - the frontmost app's, since this process
/// is not active - and `orderFrontRegardless` then puts the overlay back on
/// screen without asking for key again.
///
/// A panel that is not key is left alone; ordering it out and in would be a
/// pointless flicker.
#[cfg(target_os = "macos")]
pub fn release_key_focus(window: &WebviewWindow) -> tauri::Result<()> {
  let panel = ensure_recording_panel(window)?;
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || {
    if !panel.as_panel().isKeyWindow() {
      return;
    }
    // `Panel::hide` is `orderOut:nil` and `Panel::show` is
    // `orderFrontRegardless`; neither touches alpha, so the overlay stays as
    // visible as it was.
    panel.hide();
    panel.show();
  })
}

#[cfg(target_os = "macos")]
pub fn restore_recording_level(window: &WebviewWindow) -> tauri::Result<()> {
  let Some(level) = recording_panel_level(window) else {
    return Ok(());
  };
  let panel = ensure_recording_panel(window)?;
  panel.set_level(PanelLevel::Custom(level).value());
  Ok(())
}

#[cfg(target_os = "macos")]
pub fn raise_without_activation(window: &WebviewWindow) -> tauri::Result<()> {
  ensure_recording_panel(window)?.show();
  restore_recording_level(window)
}

#[cfg(target_os = "macos")]
pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_ignore_cursor_events(true)?;
  let Ok(panel) = registered_panel(window) else {
    return window.hide();
  };
  let window = window.clone();
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || {
    panel.set_alpha_value(0.0);
    let _ = window.hide();
    panel.hide();
  })
}

/// Orders a recording panel onscreen without disturbing keyboard focus.
///
/// Tauri's `WebviewWindow::show` is `makeKeyAndOrderFront:` underneath. On a
/// non-activating panel that is the worst of both worlds: the app never
/// activates, but WindowServer still moves keyboard focus off whatever the user
/// was working in - Final Cut, a browser - every time recording starts. So this
/// never calls it. `Panel::show` is `orderFrontRegardless`, which puts the
/// panel on screen and leaves key status where it is.
///
/// Nothing depends on the Tauri-side call: tao's `is_visible` asks the NSWindow
/// (`isVisible`), so every `window.is_visible()` caller sees the panel the
/// moment it is ordered front.
#[cfg(target_os = "macos")]
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_ignore_cursor_events(false)?;
  let panel = ensure_recording_panel(window)?;
  let app = window.app_handle().clone();
  app.run_on_main_thread(move || {
    panel.set_alpha_value(1.0);
    panel.show();
  })
}

/// Every window this app floats over the desktop is an overlay: always on top,
/// and off the taskbar. Its capture affinity follows the user's persistent
/// "record Screenwide windows" preference.
#[cfg(target_os = "windows")]
fn initialize_overlay(window: &WebviewWindow) -> tauri::Result<()> {
  window.set_always_on_top(true)?;
  window.set_skip_taskbar(true)?;
  initialize_capture_affinity(window)
}

#[cfg(target_os = "macos")]
pub fn prepare_to_show(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
}

#[cfg(target_os = "windows")]
fn initialize_capture_affinity(window: &WebviewWindow) -> tauri::Result<()> {
  let record_screenwide_windows =
    crate::settings::current(window.app_handle()).record_screenwide_windows;
  // Tauri can transiently report configured-hidden windows as visible while
  // WebView2 is creating their native surfaces. Changing display affinity in
  // that interval orders those unpainted surfaces onscreen. Hide on both sides
  // of the native call so startup never exposes a blank window shell.
  window.hide()?;
  set_capture_affinity(window, record_screenwide_windows)?;
  window.hide()
}

#[cfg(target_os = "windows")]
pub fn set_capture_affinity(
  window: &WebviewWindow,
  record_screenwide_windows: bool,
) -> tauri::Result<()> {
  use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
      GetWindowDisplayAffinity, SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    },
  };

  let hwnd = HWND(window.hwnd()?.0);
  let desired_affinity = if record_screenwide_windows {
    WDA_NONE
  } else {
    WDA_EXCLUDEFROMCAPTURE
  };
  unsafe {
    let mut current_affinity = 0;
    GetWindowDisplayAffinity(hwnd, &mut current_affinity).map_err(std::io::Error::other)?;
    if current_affinity != desired_affinity.0 {
      SetWindowDisplayAffinity(hwnd, desired_affinity).map_err(std::io::Error::other)?;
    }
  }

  Ok(())
}

#[cfg(target_os = "windows")]
pub fn is_visible(window: &WebviewWindow) -> tauri::Result<bool> {
  use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindowVisible};

  Ok(unsafe { IsWindowVisible(HWND(window.hwnd()?.0)).as_bool() })
}

#[cfg(target_os = "windows")]
pub fn prepare_to_show(window: &WebviewWindow) -> tauri::Result<()> {
  let record_screenwide_windows =
    crate::settings::current(window.app_handle()).record_screenwide_windows;
  set_capture_affinity(window, record_screenwide_windows)
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
  initialize_capture_affinity(window)
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
  prepare_to_show(window)?;
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
pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
  window.hide()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show(window: &WebviewWindow) -> tauri::Result<()> {
  window.show()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn prepare_to_show(_window: &WebviewWindow) -> tauri::Result<()> {
  Ok(())
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
