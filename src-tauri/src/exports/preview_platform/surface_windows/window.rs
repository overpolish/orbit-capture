// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Pixel-aligned geometry shared by the Windows DirectComposition surface.

/// Rounds both edges instead of rounding an origin and length independently.
/// This prevents one-pixel seams at fractional DPI and preview zoom factors.
pub(super) fn scaled_edges(origin: f64, length: f64, scale: f64) -> (i32, i32) {
  (
    (origin * scale).round() as i32,
    ((origin + length) * scale).round() as i32,
  )
}

#[cfg(test)]
mod tests {
  use super::scaled_edges;

  #[test]
  fn fractional_geometry_does_not_lose_the_far_edge() {
    let edges = scaled_edges(282.4, 908.4, 1.0);
    assert_eq!(edges, (282, 1_191));
    assert_ne!(edges.0 + 908, edges.1);
  }
}
