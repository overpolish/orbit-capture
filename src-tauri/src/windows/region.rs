// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::time::Duration;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use super::{
  collapse_recording_source_selector, platform, WindowLabel, RECORDING_CONTROLS_VISIBLE,
  REGION_SELECTOR_EDITING, SELECTOR_VISIBLE,
};

static SCREENSHOT_REGION_SESSION: AtomicBool = AtomicBool::new(false);

pub fn is_region_selector_visible(app: &AppHandle) -> bool {
  app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .is_some_and(|region| region.is_visible().unwrap_or(false))
}

#[tauri::command]
pub fn show_region_selector(
  app: AppHandle,
  position: PhysicalPosition<i32>,
  size: PhysicalSize<u32>,
) -> tauri::Result<()> {
  if !region_selector_may_show(
    crate::recording::is_idle(&app),
    RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed),
    SCREENSHOT_REGION_SESSION.load(Ordering::Relaxed),
  ) {
    return Ok(());
  }

  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  // Persisting a resize rehydrates every window's shared source store. The
  // overlay is already covering this monitor, so do not run native show/order
  // choreography again merely because its React geometry changed.
  if region.is_visible()? && region.outer_position()? == position && region.outer_size()? == size {
    return apply_region_selector_interactivity(&app);
  }
  region.set_size(size)?;
  region.set_position(position)?;
  platform::set_opacity(&region, 1.0)?;
  platform::show(&region)?;
  platform::restore_recording_level(&region)?;

  raise_recording_controls(&app)?;
  apply_region_selector_interactivity(&app)?;

  #[cfg(target_os = "macos")]
  tauri::async_runtime::spawn_blocking(move || {
    // AppKit completes showing a previously hidden panel asynchronously and
    // can order it above panels raised in the same run-loop turn.
    std::thread::sleep(Duration::from_millis(75));
    let ordering_app = app.clone();
    let _ = app.run_on_main_thread(move || {
      let Some(region) = ordering_app.get_webview_window(WindowLabel::RegionSelector.as_str())
      else {
        return;
      };
      let _ = platform::restore_recording_level(&region);
      let _ = raise_recording_controls(&ordering_app);
    });
  });

  Ok(())
}

#[tauri::command]
pub fn hide_region_selector(app: AppHandle) -> tauri::Result<()> {
  if let Some(region) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) {
    platform::hide(&region)?;
  }
  set_recording_controls_opacity(app, 1.0)
}

fn raise_recording_controls(app: &AppHandle) -> tauri::Result<()> {
  if !recording_controls_may_raise(
    RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed),
    REGION_SELECTOR_EDITING.load(Ordering::Relaxed),
  ) {
    return Ok(());
  }

  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::raise_without_activation(&bar)?;
  }
  if SELECTOR_VISIBLE.load(Ordering::Relaxed) {
    if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
      platform::raise_without_activation(&selector)?;
    }
  }
  Ok(())
}

/// Cross-window persistence may ask the overlay to show while its edit
/// gesture is ending. Editing owns the pointer during that interval, so the
/// source selector must not be raised over it.
const fn recording_controls_may_raise(controls_visible: bool, region_editing: bool) -> bool {
  controls_visible && !region_editing
}

/// Region editing owns the screen until the user explicitly finishes it.
/// Persisting resize geometry rehydrates the recording bar's source store,
/// which can legitimately re-assert that region mode has a source selector.
/// That synchronization must update the desired idle state without ordering
/// the selector back on screen in the middle of the edit gesture.
const fn source_selector_visibility_allows_show(
  controls_visible: bool,
  region_editing: bool,
) -> bool {
  controls_visible && !region_editing
}

pub(super) fn source_selector_may_show() -> bool {
  source_selector_visibility_allows_show(
    RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed),
    REGION_SELECTOR_EDITING.load(Ordering::Relaxed),
  )
}

/// The region overlay may take clicks only while the user is actively editing
/// the region, and only outside a recording. Every other time it is on screen -
/// displaying the chosen frame, or standing in as the recording boundary - it
/// has to let clicks through to whatever is underneath.
const fn region_selector_is_interactive(is_editing: bool, is_recording_idle: bool) -> bool {
  is_editing && is_recording_idle
}

/// The region boundary belongs to the recording UI while idle. Once an export
/// takes over and hides that UI, a delayed frontend synchronization must not
/// bring the overlay back by itself. An active recording may keep its existing
/// boundary visible after the controls have gone.
///
/// A screenshot session is the exception: the screenshot shortcut draws the
/// overlay on its own, with the recording controls deliberately left wherever
/// the user had them.
const fn region_selector_may_show(
  is_recording_idle: bool,
  controls_visible: bool,
  screenshot_session: bool,
) -> bool {
  !is_recording_idle || controls_visible || screenshot_session
}

/// Marks the overlay as belonging to a shortcut-initiated screenshot.
///
/// The frontend owns the session: it turns this on before asking for the
/// overlay and off once the still is taken or the session is abandoned.
#[tauri::command]
pub fn set_screenshot_region_session(active: bool) {
  SCREENSHOT_REGION_SESSION.store(active, Ordering::Relaxed);
}

/// Re-asserts that invariant against the window.
///
/// This has to be called by everything that shows the overlay, because
/// `platform::show` turns cursor events back on every time it runs. Leaving it
/// to the caller to remember is what made the desktop stop accepting clicks
/// after a re-show.
fn apply_region_selector_interactivity(app: &AppHandle) -> tauri::Result<()> {
  let Some(region) = app.get_webview_window(WindowLabel::RegionSelector.as_str()) else {
    return Ok(());
  };
  let is_interactive = region_selector_is_interactive(
    REGION_SELECTOR_EDITING.load(Ordering::Relaxed),
    crate::recording::is_idle(app),
  );

  region.set_ignore_cursor_events(!is_interactive)?;
  if is_interactive {
    region.set_focus()?;
  } else {
    #[cfg(target_os = "macos")]
    platform::resign_key(&region)?;
    raise_recording_controls(app)?;
  }

  Ok(())
}

#[tauri::command]
pub fn set_region_selector_passthrough(app: AppHandle, passthrough: bool) -> tauri::Result<()> {
  REGION_SELECTOR_EDITING.store(!passthrough, Ordering::Relaxed);
  apply_region_selector_interactivity(&app)
}

#[tauri::command]
pub fn set_region_selector_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  let region = app
    .get_webview_window(WindowLabel::RegionSelector.as_str())
    .ok_or_else(|| tauri::Error::WindowNotFound)?;
  platform::set_opacity(&region, opacity)
}

/// Fades the recording controls, and is the only thing that decides they are
/// on screen.
///
/// Fading them *in* is refused outside an idle app. The controls belong to the
/// idle state; while a recording is starting or running they are deliberately
/// gone. Several callers ask for opacity 1.0 without knowing that - hiding the
/// region overlay does it as cleanup, and the overlay window does it whenever
/// it stops editing - and `prepare_windows` itself hides the bar and then
/// hides the region overlay, which used to put the bar straight back.
#[tauri::command]
pub fn set_recording_controls_opacity(app: AppHandle, opacity: f64) -> tauri::Result<()> {
  if !RECORDING_CONTROLS_VISIBLE.load(Ordering::Relaxed) {
    return Ok(());
  }
  if opacity > 0.0 && !crate::recording::is_idle(&app) {
    return Ok(());
  }

  if let Some(bar) = app.get_webview_window(WindowLabel::RecordingBar.as_str()) {
    platform::set_opacity(&bar, opacity)?;
  }
  if SELECTOR_VISIBLE.load(Ordering::Relaxed) {
    if let Some(selector) = app.get_webview_window(WindowLabel::RecordingSourceSelector.as_str()) {
      platform::set_opacity(&selector, opacity)?;
    }
  }
  if opacity > 0.0 {
    raise_recording_controls(&app)?;
  } else {
    let _ = collapse_recording_source_selector(app.clone());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::{
    recording_controls_may_raise, region_selector_is_interactive, region_selector_may_show,
    source_selector_visibility_allows_show,
  };

  #[test]
  fn region_editing_does_not_raise_the_source_selector() {
    assert!(recording_controls_may_raise(true, false));
    assert!(!recording_controls_may_raise(true, true));
    assert!(!recording_controls_may_raise(false, false));
  }

  #[test]
  fn region_editing_keeps_the_source_selector_hidden_during_store_sync() {
    assert!(source_selector_visibility_allows_show(true, false));
    assert!(!source_selector_visibility_allows_show(true, true));
    assert!(!source_selector_visibility_allows_show(false, false));
  }

  #[test]
  fn the_region_overlay_takes_clicks_only_while_editing_outside_a_recording() {
    assert!(region_selector_is_interactive(true, true));
    assert!(!region_selector_is_interactive(false, true));
    assert!(!region_selector_is_interactive(true, false));
    assert!(!region_selector_is_interactive(false, false));
  }

  #[test]
  fn a_hidden_recording_ui_cannot_resurrect_only_its_region_overlay() {
    assert!(region_selector_may_show(true, true, false));
    assert!(region_selector_may_show(false, false, false));
    assert!(!region_selector_may_show(true, false, false));
  }

  #[test]
  fn a_screenshot_session_shows_the_overlay_without_the_recording_ui() {
    assert!(region_selector_may_show(true, false, true));
    assert!(region_selector_may_show(true, true, true));
  }
}
