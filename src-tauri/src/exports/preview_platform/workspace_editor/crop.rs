// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(any(target_os = "windows", test))]
use super::{NormalizedRect, FRAME_EDGE_BOTTOM, FRAME_EDGE_LEFT, FRAME_EDGE_RIGHT, FRAME_EDGE_TOP};

/// `f64::clamp` panics when `low > high`, and two bounds derived from the same
/// normalized geometry (`image.x + image.width - crop.width` against `image.x`
/// while the crop still spans the whole image) can differ by an ULP. An
/// inverted range collapses to `low`.
#[cfg(any(target_os = "windows", test))]
fn clamp_range(value: f64, low: f64, high: f64) -> f64 {
  value.max(low).min(high.max(low))
}

/// Move a crop window without changing the underlying image transform.
#[cfg(any(target_os = "windows", test))]
pub fn apply_crop_move(
  crop: NormalizedRect,
  image: NormalizedRect,
  delta: (f64, f64),
) -> NormalizedRect {
  let x = clamp_range(
    crop.x + delta.0,
    image.x,
    image.x + image.width - crop.width,
  );
  let y = clamp_range(
    crop.y + delta.1,
    image.y,
    image.y + image.height - crop.height,
  );
  NormalizedRect { x, y, ..crop }
}

/// Resize a crop window from edge bits (left=1, right=2, top=4, bottom=8).
/// The image bounds constrain the crop but are never modified.
#[cfg(any(target_os = "windows", test))]
pub fn apply_crop_resize(
  crop: NormalizedRect,
  image: NormalizedRect,
  edges: u32,
  delta: (f64, f64),
  centered: bool,
) -> NormalizedRect {
  let image_right = image.x + image.width;
  let image_bottom = image.y + image.height;
  let min_size = 1e-6;
  let mut left = crop.x;
  let mut right = crop.x + crop.width;
  let mut top = crop.y;
  let mut bottom = crop.y + crop.height;
  if edges & FRAME_EDGE_LEFT != 0 {
    let movement = clamp_range(delta.0, image.x - left, crop.width - min_size);
    left += movement;
    if centered {
      right -= movement;
    }
  } else if edges & FRAME_EDGE_RIGHT != 0 {
    let movement = clamp_range(delta.0, min_size - crop.width, image_right - right);
    right += movement;
    if centered {
      left -= movement;
    }
  }
  if edges & FRAME_EDGE_TOP != 0 {
    let movement = clamp_range(delta.1, image.y - top, crop.height - min_size);
    top += movement;
    if centered {
      bottom -= movement;
    }
  } else if edges & FRAME_EDGE_BOTTOM != 0 {
    let movement = clamp_range(delta.1, min_size - crop.height, image_bottom - bottom);
    bottom += movement;
    if centered {
      top -= movement;
    }
  }
  let width = (right - left).max(min_size).min(image.width.max(min_size));
  let height = (bottom - top).max(min_size).min(image.height.max(min_size));
  left = clamp_range(left, image.x, image_right - width);
  top = clamp_range(top, image.y, image_bottom - height);
  NormalizedRect {
    x: left,
    y: top,
    width,
    height,
  }
}
