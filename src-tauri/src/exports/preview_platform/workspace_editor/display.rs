// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::WorldRect;

/// A target in the coordinate space used by the native preview surface.
/// `z_order` is the back-to-front order; input order breaks equal-z ties.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayTarget {
  pub id: u64,
  pub rect: DisplayRect,
  pub radius_enabled: u8,
  pub radius_percent: f64,
  pub z_order: i32,
  pub selected: u8,
  pub visible: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayFitRebase {
  pub fit: DisplayRect,
  pub zoom: f64,
  pub pan_x: f64,
  pub pan_y: f64,
}

/// Re-express an already displayed workspace against a centered fit rect.
/// The returned zoom/pan preserves the current displayed pixels exactly.
pub fn rebase_display_fit(
  viewport: (f64, f64),
  displayed: DisplayRect,
  gutter: f64,
) -> DisplayFitRebase {
  let available_width = (viewport.0 - gutter * 2.0).max(1.0);
  let available_height = (viewport.1 - gutter * 2.0).max(1.0);
  let aspect = displayed.width.max(1.0) / displayed.height.max(1.0);
  let mut fit_width = available_width;
  let mut fit_height = fit_width / aspect.max(0.000_001);
  if fit_height > available_height {
    fit_height = available_height;
    fit_width = fit_height * aspect;
  }
  let fit = DisplayRect {
    x: (viewport.0 - fit_width) / 2.0,
    y: (viewport.1 - fit_height) / 2.0,
    width: fit_width,
    height: fit_height,
  };
  DisplayFitRebase {
    fit,
    zoom: (displayed.width / fit_width.max(1.0)).clamp(0.1, 16.0),
    pan_x: displayed.x + displayed.width / 2.0 - viewport.0 / 2.0,
    pan_y: displayed.y + displayed.height / 2.0 - viewport.1 / 2.0,
  }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayHandle {
  Body = 0,
  North = 1,
  South = 2,
  East = 3,
  West = 4,
  NorthEast = 5,
  NorthWest = 6,
  SouthEast = 7,
  SouthWest = 8,
  Radius = 9,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayHit {
  pub found: u8,
  pub target_id: u64,
  pub handle: u8,
}

impl DisplayHit {
  pub(super) fn new(target_id: u64, handle: DisplayHandle) -> Self {
    Self {
      found: 1,
      target_id,
      handle: handle as u8,
    }
  }
}

impl From<WorldRect> for DisplayRect {
  fn from(rect: WorldRect) -> Self {
    Self {
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    }
  }
}
