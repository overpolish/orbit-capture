// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::OnceLock;

use image::RgbaImage;
use objc2_app_kit::NSCursor;

use crate::recording::cursor::CursorStyle;

struct SystemArtwork(Vec<(CursorStyle, RgbaImage)>);

static ARTWORK: OnceLock<Result<SystemArtwork, String>> = OnceLock::new();

pub(super) fn initialize() {
  if let Err(error) = ARTWORK.get_or_init(load) {
    eprintln!("Could not load macOS cursor artwork: {error}");
  }
}

pub(super) fn artwork(style: CursorStyle) -> Option<&'static RgbaImage> {
  ARTWORK
    .get()
    .and_then(|result| result.as_ref().ok())
    .and_then(|artwork| {
      artwork
        .0
        .iter()
        .find_map(|(candidate, image)| (*candidate == canonical_style(style)).then_some(image))
    })
}

pub(super) fn canonical_style(style: CursorStyle) -> CursorStyle {
  if style == CursorStyle::Custom {
    CursorStyle::Arrow
  } else {
    style
  }
}

fn load() -> Result<SystemArtwork, String> {
  let cursors = [
    (CursorStyle::Arrow, NSCursor::arrowCursor()),
    (CursorStyle::ClosedHand, NSCursor::closedHandCursor()),
    (CursorStyle::ContextMenu, NSCursor::contextualMenuCursor()),
    (CursorStyle::Crosshair, NSCursor::crosshairCursor()),
    (
      CursorStyle::DisappearingItem,
      NSCursor::disappearingItemCursor(),
    ),
    (CursorStyle::DragCopy, NSCursor::dragCopyCursor()),
    (CursorStyle::DragLink, NSCursor::dragLinkCursor()),
    (CursorStyle::IBeam, NSCursor::IBeamCursor()),
    (
      CursorStyle::NotAllowed,
      NSCursor::operationNotAllowedCursor(),
    ),
    (CursorStyle::OpenHand, NSCursor::openHandCursor()),
    (CursorStyle::PointingHand, NSCursor::pointingHandCursor()),
    (
      CursorStyle::ResizeHorizontal,
      NSCursor::columnResizeCursor(),
    ),
    (CursorStyle::ResizeVertical, NSCursor::rowResizeCursor()),
    (
      CursorStyle::VerticalIBeam,
      NSCursor::IBeamCursorForVerticalLayout(),
    ),
    (CursorStyle::ZoomIn, NSCursor::zoomInCursor()),
    (CursorStyle::ZoomOut, NSCursor::zoomOutCursor()),
  ];
  cursors
    .into_iter()
    .map(|(style, cursor)| {
      let data = cursor
        .image()
        .TIFFRepresentation()
        .ok_or_else(|| format!("The {style:?} system cursor has no artwork"))?;
      let image = image::load_from_memory(&data.to_vec())
        .map_err(|error| format!("Could not decode the {style:?} system cursor: {error}"))?
        .into_rgba8();
      Ok((style, image))
    })
    .collect::<Result<Vec<_>, String>>()
    .map(SystemArtwork)
}
