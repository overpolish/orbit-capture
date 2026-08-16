// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::{AppHandle, Manager};

use super::{window, ExportArtifact, ExportState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptureWorkspaceReservation {
  Recording,
  Screenshot,
}

pub(super) fn focus_pending(app: &AppHandle) {
  let _ = window::show(app);
}

pub fn has_pending(app: &AppHandle) -> bool {
  app
    .state::<ExportState>()
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .is_some()
}

/// Focuses pending work and reports whether the requested capture should stop.
pub fn focus_if_pending(app: &AppHandle) -> bool {
  let pending = has_pending(app);
  if pending {
    focus_pending(app);
  }
  pending
}

/// Screenshot tools may open over an existing screenshot canvas. Only a
/// recording waiting for export, or a capture already in flight, blocks a new
/// screenshot interaction.
pub fn focus_if_screenshot_blocked(app: &AppHandle) -> bool {
  let state = app.state::<ExportState>();
  let recording_waits = matches!(
    state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .as_ref(),
    Some(ExportArtifact::Recording { .. })
  );
  let capture_waits = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .is_some();
  let blocked = recording_waits || capture_waits;
  if blocked {
    focus_pending(app);
  }
  blocked
}

/// Reserves the empty workspace before countdown and stream initialization.
/// Every recording entry point therefore sees the same pending-work rule.
pub fn reserve_recording(app: &AppHandle) -> Result<(), String> {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let mut reservation = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  if artifact.is_some() || reservation.is_some() {
    focus_pending(app);
    Err("Finish or discard the open export before starting another recording".to_owned())
  } else {
    *reservation = Some(CaptureWorkspaceReservation::Recording);
    Ok(())
  }
}

/// Screenshots append to an open screenshot workspace, but never replace an
/// unsaved recording.
pub fn reserve_screenshot(app: &AppHandle, clipboard_only: bool) -> Result<(), String> {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let mut reservation = state
    .capture_reservation
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let recording_waits = matches!(artifact.as_ref(), Some(ExportArtifact::Recording { .. }));
  let capture_waits = reservation.is_some();
  let pending_export_blocks = !clipboard_only && recording_waits;
  if pending_export_blocks || capture_waits {
    focus_pending(app);
    Err(if capture_waits {
      "Another capture is already starting".to_owned()
    } else if recording_waits {
      "Finish or discard the open recording before taking a screenshot".to_owned()
    } else {
      unreachable!("a blocked screenshot has a reservation or pending export")
    })
  } else {
    *reservation = Some(CaptureWorkspaceReservation::Screenshot);
    Ok(())
  }
}

fn release(app: &AppHandle, expected: CaptureWorkspaceReservation) {
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
  release(app, CaptureWorkspaceReservation::Recording);
}

pub fn release_screenshot(app: &AppHandle) {
  release(app, CaptureWorkspaceReservation::Screenshot);
}
