// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
  display::{rebase_display_fit, DisplayFitRebase, DisplayHit, DisplayRect, DisplayTarget},
  hit_test::hit_test_display,
};

#[no_mangle]
pub extern "C" fn screenwide_workspace_rebase_display_fit(
  viewport_width: f64,
  viewport_height: f64,
  displayed: DisplayRect,
  gutter: f64,
) -> DisplayFitRebase {
  rebase_display_fit((viewport_width, viewport_height), displayed, gutter)
}

/// C entry point for native adapters. Null pointers and invalid counts return no hit.
#[no_mangle]
pub unsafe extern "C" fn screenwide_workspace_hit_test(
  targets: *const DisplayTarget,
  count: usize,
  x: f64,
  y: f64,
  handle_size: f64,
) -> DisplayHit {
  if targets.is_null() || count == 0 {
    return DisplayHit::default();
  }
  let targets = unsafe { std::slice::from_raw_parts(targets, count) };
  hit_test_display(targets, (x, y), handle_size).unwrap_or_default()
}
