// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{DisplayHandle, DisplayHit, DisplayRect, DisplayTarget};

/// Hit-test display-space targets. Only the selected target exposes handle
/// hit regions because it is the only target whose handles are visible.
/// Those handles own their full hit squares before layer bodies, including
/// the portion extending outside the selected layer. Unselected targets are
/// selectable through their bodies without exposing invisible point OSCs.
pub fn hit_test_display(
  targets: &[DisplayTarget],
  point: (f64, f64),
  handle_size: f64,
) -> Option<DisplayHit> {
  let radius = handle_size.max(0.0);
  let mut order: Vec<(usize, &DisplayTarget)> = targets
    .iter()
    .enumerate()
    .filter(|(_, t)| t.visible != 0)
    .collect();
  order.sort_by_key(|(index, target)| (target.z_order, *index));
  // The handles actually drawn for the selected target own their full hit
  // squares, even where those squares overlap a neighbouring layer or one of
  // its currently invisible handles.
  for (_, target) in order
    .iter()
    .rev()
    .filter(|(_, target)| target.selected != 0)
  {
    if let Some(handle) = edge_handle(target.rect, point, radius) {
      return Some(DisplayHit::new(target.id, handle));
    }
  }
  if let Some((_, target)) = order.iter().rev().find(|(_, target)| {
    target.selected != 0
      && target.radius_enabled != 0
      && radius_hit(target.rect, target.radius_percent, point, radius)
  }) {
    return Some(DisplayHit::new(target.id, DisplayHandle::Radius));
  }
  order
    .iter()
    .rev()
    .find(|(_, target)| contains(target.rect, point))
    .map(|(_, target)| DisplayHit::new(target.id, DisplayHandle::Body))
}

fn contains(rect: DisplayRect, point: (f64, f64)) -> bool {
  point.0 >= rect.x
    && point.1 >= rect.y
    && point.0 <= rect.x + rect.width
    && point.1 <= rect.y + rect.height
}

fn edge_handle(rect: DisplayRect, point: (f64, f64), size: f64) -> Option<DisplayHandle> {
  let points = [
    (rect.x, rect.y, DisplayHandle::NorthWest),
    (rect.x + rect.width / 2.0, rect.y, DisplayHandle::North),
    (rect.x + rect.width, rect.y, DisplayHandle::NorthEast),
    (
      rect.x + rect.width,
      rect.y + rect.height / 2.0,
      DisplayHandle::East,
    ),
    (
      rect.x + rect.width,
      rect.y + rect.height,
      DisplayHandle::SouthEast,
    ),
    (
      rect.x + rect.width / 2.0,
      rect.y + rect.height,
      DisplayHandle::South,
    ),
    (rect.x, rect.y + rect.height, DisplayHandle::SouthWest),
    (rect.x, rect.y + rect.height / 2.0, DisplayHandle::West),
  ];
  points
    .into_iter()
    .find(|(x, y, _)| (point.0 - x).abs() <= size && (point.1 - y).abs() <= size)
    .map(|(_, _, handle)| handle)
}

fn radius_hit(rect: DisplayRect, percent: f64, point: (f64, f64), size: f64) -> bool {
  let offset = rect.width.min(rect.height) * percent.clamp(0.0, 50.0) / 100.0 * 0.55 + 10.0;
  (point.0 - rect.x - offset).abs() <= size && (point.1 - rect.y - offset).abs() <= size
}
