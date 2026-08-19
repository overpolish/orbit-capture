// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Manager};

use super::{window, ExportKind, ExportState};

pub(super) fn focus_pending(app: &AppHandle, kind: ExportKind) {
  let _ = window::show(app, kind);
}

pub fn has_pending_kind(app: &AppHandle, kind: ExportKind) -> bool {
  app
    .state::<ExportState>()
    .slot(kind)
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .is_some()
}

/// Whether any workspace is holding unsaved work.
pub fn has_pending(app: &AppHandle) -> bool {
  ExportKind::ALL
    .into_iter()
    .any(|kind| has_pending_kind(app, kind))
}

/// Focuses a pending recording and reports whether the requested action should
/// stop. Only the recording workspace is consulted: an open screenshot never
/// stood in the way of the recording controls, and now it has its own window
/// that would be the wrong one to raise.
pub fn focus_if_pending(app: &AppHandle) -> bool {
  let pending = has_pending_kind(app, ExportKind::Recording);
  if pending {
    focus_pending(app, ExportKind::Recording);
  }
  pending
}

/// Screenshot tools open over whatever else is waiting: each workspace has its
/// own window, so only a capture already in flight blocks a new one.
pub fn focus_if_screenshot_blocked(app: &AppHandle) -> bool {
  let state = app.state::<ExportState>();
  let reservation = *state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(kind) = reservation else {
    return false;
  };
  // A reservation of its own has nothing to show yet; raise its window only if
  // that workspace already holds something the user can act on.
  if has_pending_kind(app, kind) {
    focus_pending(app, kind);
  }
  true
}

/// Reserves the empty recording workspace before countdown and stream
/// initialization. Every recording entry point therefore sees the same
/// pending-work rule.
pub fn reserve_recording(app: &AppHandle) -> Result<(), String> {
  let state = app.state::<ExportState>();
  let artifact = state
    .recording
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let mut reservation = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  if artifact.is_some() {
    drop(reservation);
    drop(artifact);
    focus_pending(app, ExportKind::Recording);
    Err("Finish or discard the open recording before starting another".to_owned())
  } else if reservation.is_some() {
    Err("Another capture is already starting".to_owned())
  } else {
    *reservation = Some(ExportKind::Recording);
    Ok(())
  }
}

/// Screenshots append to an open screenshot workspace and are indifferent to a
/// recording waiting in its own window. Only a capture already being set up
/// stands in the way.
pub fn reserve_screenshot(app: &AppHandle) -> Result<(), String> {
  let state = app.state::<ExportState>();
  let mut reservation = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  if reservation.is_some() {
    Err("Another capture is already starting".to_owned())
  } else {
    *reservation = Some(ExportKind::Screenshot);
    Ok(())
  }
}

fn release(app: &AppHandle, expected: ExportKind) {
  let state = app.state::<ExportState>();
  let mut reservation = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  if *reservation == Some(expected) {
    *reservation = None;
  }
}

pub fn release_recording(app: &AppHandle) {
  release(app, ExportKind::Recording);
}

pub fn release_screenshot(app: &AppHandle) {
  release(app, ExportKind::Screenshot);
}
