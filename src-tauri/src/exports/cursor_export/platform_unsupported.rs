// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

pub(super) fn export(_request: CursorExportRequest<'_>) -> Result<ExportRunResult, String> {
  Err("Cursor baking is not available on this platform yet".to_owned())
}
