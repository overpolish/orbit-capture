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

#[cfg(target_os = "windows")]
#[test]
fn custom_windows_cursor_uses_the_fallback_arrows_tip() {
  let custom = Appearance {
    hotspot_x: 13.0,
    hotspot_y: 13.0,
    ..appearance(0, CursorStyle::Custom)
  };
  assert_eq!(output_hotspot(custom), (0.0, 0.0));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_standard_cursors_keep_their_recorded_native_hotspots() {
  let assert_hotspot = |actual: (f64, f64), expected: (f64, f64)| {
    assert!((actual.0 - expected.0).abs() < f64::EPSILON * 16.0);
    assert!((actual.1 - expected.1).abs() < f64::EPSILON * 16.0);
  };
  let vector = |style| Appearance {
    height: 32.0,
    hotspot_x: 8.0,
    hotspot_y: 9.0,
    style,
    timestamp_us: 0,
    width: 32.0,
  };
  assert_hotspot(output_hotspot(vector(CursorStyle::IBeam)), (8.0, 9.0));
  assert_hotspot(
    output_hotspot(vector(CursorStyle::ResizeHorizontal)),
    (8.0, 9.0),
  );
  assert_hotspot(
    output_hotspot(vector(CursorStyle::PointingHand)),
    (8.0, 9.0),
  );
}

#[cfg(target_os = "windows")]
#[test]
fn windows_cursor_position_has_no_macos_screen_reaction_delay() {
  assert_eq!(SCREEN_REACTION_US, 0);
}
