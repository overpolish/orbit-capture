// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The folder the next export lands in: whatever was used last, falling back to
/// the platform's own screenshot folder on a first run.
pub(super) fn current_directory(app: &AppHandle, kind: ExportKind) -> Option<PathBuf> {
  let state = app.state::<ExportState>();
  let remembered = state
    .slot(kind)
    .directory
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
    .clone();

  remembered.or_else(|| screenshot_directory(app).ok())
}
