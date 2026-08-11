// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Mutex;

use core_graphics::{
  display::CGDisplay,
  event::CGEvent,
  event_source::{CGEventSource, CGEventSourceStateID},
  geometry::CGPoint,
};

static CURSOR_ANCHOR: Mutex<Option<(f64, f64)>> = Mutex::new(None);

#[tauri::command]
pub fn begin_cursor_scrub() -> Result<(), String> {
  let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
    .map_err(|()| "Could not read the cursor position".to_string())?;
  let point = CGEvent::new(source)
    .map_err(|()| "Could not read the cursor position".to_string())?
    .location();

  CGDisplay::associate_mouse_and_mouse_cursor_position(false)
    .map_err(|error| format!("Could not pin the cursor: {error}"))?;
  if let Err(error) = CGDisplay::warp_mouse_cursor_position(point) {
    let _ = CGDisplay::associate_mouse_and_mouse_cursor_position(true);
    return Err(format!("Could not pin the cursor: {error}"));
  }

  *CURSOR_ANCHOR
    .lock()
    .map_err(|_| "Could not store the cursor position".to_string())? = Some((point.x, point.y));
  Ok(())
}

#[tauri::command]
pub fn end_cursor_scrub() -> Result<(), String> {
  let anchor = CURSOR_ANCHOR
    .lock()
    .map_err(|_| "Could not restore the cursor position".to_string())?
    .take();

  let warp_result = anchor
    .map(|(x, y)| CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y)))
    .transpose();
  let association_result = CGDisplay::associate_mouse_and_mouse_cursor_position(true);

  warp_result
    .and(association_result)
    .map_err(|error| format!("Could not restore the cursor position: {error}"))
}
