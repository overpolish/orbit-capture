// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use image::RgbaImage;

use crate::recording::cursor::CursorStyle;

pub(super) fn initialize() {}

pub(super) fn artwork(_style: CursorStyle) -> Option<&'static RgbaImage> {
  None
}

pub(super) fn canonical_style(style: CursorStyle) -> CursorStyle {
  style
}
