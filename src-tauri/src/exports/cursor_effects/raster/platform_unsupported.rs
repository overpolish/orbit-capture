// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use image::RgbaImage;

use crate::recording::cursor::CursorStyle;

pub(super) fn initialize() {}

/// Mirrors the macOS entry so the shared raster can ask for a style's bitmap
/// and the hotspot that bitmap addresses without platform branches.
pub(super) struct StyleArtwork {
  pub hotspot_x: f64,
  pub hotspot_y: f64,
  pub image: RgbaImage,
}

pub(super) fn style_artwork(_style: CursorStyle) -> Option<&'static StyleArtwork> {
  None
}

pub(super) fn artwork(_style: CursorStyle) -> Option<&'static RgbaImage> {
  None
}

pub(super) fn canonical_style(style: CursorStyle) -> CursorStyle {
  style
}
