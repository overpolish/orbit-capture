// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Focus management for the per-monitor ruler windows: focus follows the
//! cursor between them, and losing focus to anything else ends the session.

use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  time::Duration,
};

use tauri::{AppHandle, Manager, WindowEvent};

use super::{dismiss, ruler_windows, SCREENSHOT_MODE};

/// Focus moving between the per-monitor ruler windows arrives as a blur and a
/// focus event in either order, so a blur only decides the session's fate after
/// the sibling window has had time to claim focus.
const FOCUS_SETTLE: Duration = Duration::from_millis(180);

/// How often the cursor is matched against monitor bounds to move key focus.
const FOCUS_POLL: Duration = Duration::from_millis(120);

/// One monitor's rect in the same coordinate space as [`poll_cursor`]: logical
/// on macOS (per-monitor "physical" rects do not share a global space when
/// scale factors differ), physical elsewhere.
pub struct FocusRegion {
  pub label: String,
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

/// The cursor in the coordinate space of [`FocusRegion`]. On macOS the global
/// cursor is reported in primary-scale physical coordinates (verified by
/// instrumentation: logical position times the primary scale factor), so it is
/// divided back down to logical before containment tests.
fn poll_cursor(app: &AppHandle) -> Option<(f64, f64)> {
  let cursor = app.cursor_position().ok()?;
  #[cfg(target_os = "macos")]
  {
    let scale = app
      .primary_monitor()
      .ok()
      .flatten()
      .map_or(1.0, |monitor| monitor.scale_factor());
    Some((cursor.x / scale, cursor.y / scale))
  }
  #[cfg(not(target_os = "macos"))]
  {
    Some((cursor.x, cursor.y))
  }
}

/// macOS delivers mouse events only to the key window, so an unfocused ruler
/// webview sees nothing until it is clicked (verified by instrumentation).
/// Focus must therefore follow the cursor natively: while the session is live,
/// key focus moves to whichever monitor's ruler window contains the pointer.
/// Focus is only ever shuttled BETWEEN ruler windows — if none of them holds
/// it, the user has switched away and `watch_focus` is about to end the
/// session, so grabbing focus back would fight the dismissal.
pub fn follow_cursor_focus(app: &AppHandle, regions: Vec<FocusRegion>) {
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    loop {
      tokio::time::sleep(FOCUS_POLL).await;
      if SCREENSHOT_MODE.load(Ordering::Relaxed) {
        continue;
      }
      let windows = ruler_windows(&app);
      if windows.is_empty() {
        return;
      }
      if !windows
        .iter()
        .any(|window| window.is_focused().unwrap_or(false))
      {
        continue;
      }
      let Some((x, y)) = poll_cursor(&app) else {
        continue;
      };
      let Some(region) = regions.iter().find(|region| {
        x >= region.x
          && x < region.x + region.width
          && y >= region.y
          && y < region.y + region.height
      }) else {
        continue;
      };
      let Some(target) = windows.iter().find(|window| window.label() == region.label) else {
        continue;
      };
      if target.is_focused().unwrap_or(true) {
        continue;
      }
      let _ = target.set_focus();
    }
  });
}

/// Cancels the session once focus has left every ruler window. The event handler
/// itself runs on the main thread, so the re-check is deferred onto the async
/// runtime rather than slept on where it would freeze the UI.
pub fn watch_focus(window: &tauri::WebviewWindow) {
  let app = window.app_handle().clone();
  let focused_once = Arc::new(AtomicBool::new(false));
  window.on_window_event(move |event| {
    let WindowEvent::Focused(focused) = event else {
      return;
    };
    if *focused {
      focused_once.store(true, Ordering::Relaxed);
      return;
    }
    // Windows that never held focus (every monitor but the first, while the
    // session is still opening) have no focus to lose.
    if !focused_once.load(Ordering::Relaxed) || SCREENSHOT_MODE.load(Ordering::Relaxed) {
      return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
      tokio::time::sleep(FOCUS_SETTLE).await;
      if SCREENSHOT_MODE.load(Ordering::Relaxed) {
        return;
      }
      let windows = ruler_windows(&app);
      if windows.is_empty()
        || windows
          .iter()
          .any(|window| window.is_focused().unwrap_or(false))
      {
        return;
      }
      dismiss(&app);
    });
  });
}
