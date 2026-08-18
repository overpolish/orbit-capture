// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::OnceLock;

use image::RgbaImage;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageRep, NSCursor, NSImage, NSImageRep};
use objc2_foundation::NSArray;

use crate::recording::cursor::CursorStyle;

/// One system cursor's bitmap plus the hotspot that bitmap addresses. The
/// hotspot is stored in bitmap texels rather than points so callers can anchor
/// the artwork without knowing what backing scale the decoded representation
/// came from — `load` picks the largest representation the system ships, so
/// that scale is whatever that representation carries (10x for the arrow, 2x
/// for most styles) and never assumed to be 1x.
pub(super) struct StyleArtwork {
  pub hotspot_x: f64,
  pub hotspot_y: f64,
  pub image: RgbaImage,
}

struct SystemArtwork(Vec<(CursorStyle, StyleArtwork)>);

static ARTWORK: OnceLock<Result<SystemArtwork, String>> = OnceLock::new();

pub(super) fn initialize() {
  if let Err(error) = ARTWORK.get_or_init(load) {
    eprintln!("Could not load macOS cursor artwork: {error}");
  }
}

pub(super) fn style_artwork(style: CursorStyle) -> Option<&'static StyleArtwork> {
  ARTWORK
    .get()
    .and_then(|result| result.as_ref().ok())
    .and_then(|artwork| {
      artwork
        .0
        .iter()
        .find_map(|(candidate, entry)| (*candidate == canonical_style(style)).then_some(entry))
    })
}

pub(super) fn artwork(style: CursorStyle) -> Option<&'static RgbaImage> {
  style_artwork(style).map(|entry| &entry.image)
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
      let native = cursor.image();
      let image = decode_largest_representation(&native)
        .or_else(|| decode_flattened(&native))
        .ok_or_else(|| format!("Could not decode the {style:?} system cursor"))?;
      // `hotSpot` is expressed in the image's points with the origin at its
      // top-left corner, the same convention the rasters sample in, so it only
      // needs the decoded representation's backing scale applied. `size` stays
      // the point size whichever representation was decoded, so this scales
      // naturally with the larger bitmaps.
      let size = native.size();
      let hotspot = cursor.hotSpot();
      let (hotspot_x, hotspot_y) = hotspot_texels(
        (hotspot.x, hotspot.y),
        (size.width, size.height),
        (image.width(), image.height()),
      );
      Ok((
        style,
        StyleArtwork {
          hotspot_x,
          hotspot_y,
          image,
        },
      ))
    })
    .collect::<Result<Vec<_>, String>>()
    .map(SystemArtwork)
}

/// Bitmap texels per point, applied to the hotspot. Scale-invariant by
/// construction: whatever backing scale the decoded representation carries,
/// `pixels / points` recovers it, so a 2x representation puts the hotspot at
/// twice the point coordinate and a 10x one at ten times.
fn hotspot_texels(
  hotspot: (f64, f64),
  point_size: (f64, f64),
  pixel_size: (u32, u32),
) -> (f64, f64) {
  let texels_per_point_x = if point_size.0 > 0.0 {
    f64::from(pixel_size.0) / point_size.0
  } else {
    1.0
  };
  let texels_per_point_y = if point_size.1 > 0.0 {
    f64::from(pixel_size.1) / point_size.1
  } else {
    1.0
  };
  (
    hotspot.0 * texels_per_point_x,
    hotspot.1 * texels_per_point_y,
  )
}

/// The cursor's artwork at the highest resolution the system ships.
///
/// A system cursor's `NSImage` carries several representations (the arrow ships
/// 28x40, 56x80, 140x200 and 280x400), and exports composite at retina and
/// above, so anything below the largest visibly softens. `TIFFRepresentation`
/// flattens all of them into one multi-page TIFF and `image::load_from_memory`
/// decodes a single page; which page that is depends on the order AppKit writes
/// them in, which is documented nowhere. Measured on macOS 15 it happens to be
/// the largest for all sixteen styles, but nothing guarantees it. Picking the
/// representation with the most pixels and re-encoding only that one makes the
/// choice explicit: the TIFF handed to the decoder has exactly one page, so the
/// sharpest artwork is the only thing it can produce.
///
/// Cost: 16 styles at up to ~280x400 RGBA is ~450KB each, ~7MB in total, loaded
/// once into the process and uploaded once into the GPU artwork texture array
/// (which sizes its slices to the largest artwork) — the same bitmaps the
/// flattened decode already yielded, so this is not new spend.
fn decode_largest_representation(native: &NSImage) -> Option<RgbaImage> {
  let representations = native.representations();
  let largest = representations
    .iter()
    .filter(|representation| representation.pixelsWide() > 0 && representation.pixelsHigh() > 0)
    .max_by_key(|representation| {
      (representation.pixelsWide() as i128) * (representation.pixelsHigh() as i128)
    })?;
  // Re-encoding one representation on its own produces a single-page TIFF, so
  // the decoder below cannot silently pick a smaller page. This goes through
  // `NSBitmapImageRep` rather than a downcast so representations that are not
  // themselves bitmaps still rasterise at their own pixel dimensions.
  let single = NSArray::from_slice(&[&*largest as &NSImageRep]);
  let data = NSBitmapImageRep::TIFFRepresentationOfImageRepsInArray(&single)?;
  let image = image::load_from_memory(&data.to_vec()).ok()?.into_rgba8();
  (image.width() > 0 && image.height() > 0).then_some(image)
}

/// The original whole-image decode, kept as the fallback for any style whose
/// representations could not be enumerated or re-encoded. Whichever page it
/// lands on is correct if possibly soft: every consumer derives its scale from
/// the bitmap's own dimensions, never from an assumed 1x.
fn decode_flattened(native: &NSImage) -> Option<RgbaImage> {
  let data: Retained<objc2_foundation::NSData> = native.TIFFRepresentation()?;
  let image = image::load_from_memory(&data.to_vec()).ok()?.into_rgba8();
  (image.width() > 0 && image.height() > 0).then_some(image)
}

#[cfg(test)]
mod tests {
  use super::hotspot_texels;

  /// Pins the invariant the larger representations depend on: the hotspot is
  /// stored in bitmap texels, so it tracks the decoded bitmap's backing scale
  /// rather than the 1x point size. A 2x representation of a 28x40pt arrow
  /// whose hotspot sits at (4, 4)pt must address texel (8, 8).
  #[test]
  fn hotspot_scales_with_the_decoded_representation() {
    assert_eq!(
      hotspot_texels((4.0, 4.0), (28.0, 40.0), (56, 80)),
      (8.0, 8.0)
    );
    // The same artwork at 1x and at 10x: same point hotspot, texels scale.
    assert_eq!(
      hotspot_texels((4.0, 4.0), (28.0, 40.0), (28, 40)),
      (4.0, 4.0)
    );
    assert_eq!(
      hotspot_texels((4.0, 4.0), (28.0, 40.0), (280, 400)),
      (40.0, 40.0)
    );
  }

  /// A degenerate point size must not divide by zero; the hotspot passes
  /// through in texels.
  #[test]
  fn hotspot_survives_a_zero_point_size() {
    assert_eq!(hotspot_texels((3.0, 5.0), (0.0, 0.0), (16, 16)), (3.0, 5.0));
  }
}
