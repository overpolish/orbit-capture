// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::recording::PrimaryRecordingKind;

pub(super) fn validate_resolution_scale(selected: u16, source: u16) -> Result<(), String> {
  if selected < 100 || selected > source {
    return Err("The selected output resolution is not available for this recording".to_owned());
  }

  Ok(())
}

pub(super) fn validate_camera_resolution_scale(selected: u16) -> Result<(), String> {
  if ![50, 75, 100].contains(&selected) {
    return Err("The selected camera resolution is not available".to_owned());
  }

  Ok(())
}

pub(super) fn validate_primary_resolution_scale(
  selected: u16,
  source: u16,
  kind: PrimaryRecordingKind,
) -> Result<(), String> {
  match kind {
    PrimaryRecordingKind::Camera => validate_camera_resolution_scale(selected),
    PrimaryRecordingKind::Screen => validate_resolution_scale(selected, source),
  }
}
