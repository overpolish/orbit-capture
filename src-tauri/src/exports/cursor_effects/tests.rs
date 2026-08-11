// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn appearance(timestamp_us: u64, style: CursorStyle) -> Appearance {
  Appearance {
    height: 32.0,
    hotspot_x: 1.0,
    hotspot_y: 1.0,
    style,
    timestamp_us,
    width: 24.0,
  }
}

#[test]
fn ignores_brief_style_changes_without_delaying_a_stable_one() {
  let appearances = [
    appearance(0, CursorStyle::Arrow),
    appearance(100_000, CursorStyle::IBeam),
    appearance(320_000, CursorStyle::Arrow),
    appearance(500_000, CursorStyle::IBeam),
    appearance(900_000, CursorStyle::Arrow),
  ];
  let stable = stable_appearances(&appearances, 1_000_000);
  assert_eq!(stable.len(), 2);
  assert_eq!(stable[0].style, CursorStyle::Arrow);
  assert_eq!(stable[1].style, CursorStyle::IBeam);
  assert_eq!(stable[1].timestamp_us, 500_000);
}

#[test]
fn motion_blur_samples_never_leave_visible_gaps() {
  for distance in [2.0, 12.0, 40.0, MAX_BLUR_DISTANCE] {
    let samples = motion_blur_sample_count(distance);
    let spacing = distance / (samples - 1) as f64;
    assert!(spacing <= 2.0, "{distance}px blur left {spacing}px gaps");
  }
}
