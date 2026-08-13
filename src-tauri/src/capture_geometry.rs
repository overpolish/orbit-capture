// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::recording::Region;

/// A capture rectangle in physical device pixels, relative to its monitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CaptureRect {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
}

/// Converts a logical, monitor-local region into physical device pixels.
///
/// The two platforms disagree about units - ScreenCaptureKit's source rect is
/// in points, while Windows Graphics Capture deals in physical pixels - so
/// everything is normalised to physical here, exactly once. Edges are rounded
/// before the size is derived from them, and the result is clamped to the
/// monitor because neither native capture adapter should receive invalid
/// geometry.
pub(crate) fn physical_capture_rect(
  region: Region,
  scale: f64,
  monitor_width: u32,
  monitor_height: u32,
) -> Option<CaptureRect> {
  let edges = [
    region.position.x,
    region.position.y,
    region.size.width,
    region.size.height,
    scale,
  ];
  if !edges.iter().all(|edge| edge.is_finite()) || scale <= 0.0 {
    return None;
  }

  let monitor_width = f64::from(monitor_width);
  let monitor_height = f64::from(monitor_height);
  let left = (region.position.x * scale)
    .round()
    .clamp(0.0, monitor_width);
  let top = (region.position.y * scale)
    .round()
    .clamp(0.0, monitor_height);
  let right = ((region.position.x + region.size.width) * scale)
    .round()
    .clamp(0.0, monitor_width);
  let bottom = ((region.position.y + region.size.height) * scale)
    .round()
    .clamp(0.0, monitor_height);

  if right <= left || bottom <= top {
    return None;
  }

  Some(CaptureRect {
    x: left as u32,
    y: top as u32,
    width: (right - left) as u32,
    height: (bottom - top) as u32,
  })
}

/// NV12/H.264/HEVC encode chroma in two-pixel blocks. Preserve the selected
/// top-left edge and trim at most one physical pixel from the bottom/right so
/// the native capture stream and encoder always agree about their dimensions.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) const fn video_capture_rect(mut rect: CaptureRect) -> Option<CaptureRect> {
  rect.width &= !1;
  rect.height &= !1;
  if rect.width == 0 || rect.height == 0 {
    None
  } else {
    Some(rect)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn video_keeps_the_origin_and_trims_only_odd_far_edges() {
    let rect = video_capture_rect(CaptureRect {
      x: 11,
      y: 13,
      width: 301,
      height: 201,
    })
    .unwrap();

    assert_eq!(
      rect,
      CaptureRect {
        x: 11,
        y: 13,
        width: 300,
        height: 200,
      }
    );
  }

  #[test]
  fn video_rejects_a_region_smaller_than_one_chroma_block() {
    assert_eq!(
      video_capture_rect(CaptureRect {
        x: 0,
        y: 0,
        width: 1,
        height: 2,
      }),
      None
    );
  }
}
