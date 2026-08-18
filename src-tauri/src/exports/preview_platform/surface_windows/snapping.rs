// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native selection snapping shared by the Windows screenshot and recording editors.

#[derive(Clone, Copy, Default)]
pub(super) struct SnapGuide {
  pub found: bool,
  pub object: bool,
  pub adjustment: f64,
  pub distance: f64,
  pub guide: f64,
}

fn consider(best: &mut SnapGuide, adjustment: f64, guide: f64, object: bool, threshold: f64) {
  let distance = adjustment.abs();
  if distance > threshold
    || (best.found && (distance > best.distance || (distance == best.distance && !object)))
  {
    return;
  }
  *best = SnapGuide {
    found: true,
    object,
    adjustment,
    distance,
    guide,
  };
}

pub(super) fn move_axis(
  position: f64,
  extent: f64,
  pane_width: f64,
  pane_height: f64,
  zoom: f64,
  targets: &[(u32, f64, f64)],
  layer_id: u32,
) -> SnapGuide {
  let mut best = SnapGuide::default();
  let threshold = 8.0 / (pane_width * zoom).max(1.0);
  let inset = pane_width.min(pane_height) * 0.02 / pane_width.max(1.0);
  let maximum = 1.0 - extent;
  let placements = [
    if maximum >= 0.0 {
      inset.min(maximum)
    } else {
      0.0
    },
    maximum / 2.0,
    if maximum >= 0.0 {
      (maximum - inset).max(0.0)
    } else {
      maximum
    },
  ];
  for (index, placement) in placements.into_iter().enumerate() {
    let guide = if index == 0 {
      placement
    } else if index == 1 {
      0.5
    } else {
      placement + extent
    };
    consider(&mut best, placement - position, guide, false, threshold);
  }
  let moving = [position, position + extent / 2.0, position + extent];
  for &(target_layer, origin, target_extent) in targets {
    if target_layer == layer_id {
      continue;
    }
    let target_edges = [origin, origin + target_extent / 2.0, origin + target_extent];
    for moving_edge in moving {
      for target_edge in target_edges {
        consider(
          &mut best,
          target_edge - moving_edge,
          target_edge,
          true,
          threshold,
        );
      }
    }
  }
  best
}

pub(super) fn resize_axis(
  anchor: f64,
  vector: f64,
  raw_scale: f64,
  pane_width: f64,
  pane_height: f64,
  zoom: f64,
  minimum: f64,
  maximum: f64,
  targets: &[(u32, f64, f64)],
  layer_id: u32,
) -> SnapGuide {
  let mut best = SnapGuide::default();
  if vector.abs() < 0.0000001 {
    return best;
  }
  let threshold = 8.0 / (pane_width * zoom).max(1.0);
  let handle = anchor + vector * raw_scale;
  let inset = pane_width.min(pane_height) * 0.02 / pane_width.max(1.0);
  let canvas = [inset, 0.5, 1.0 - inset];
  for guide in canvas {
    let candidate = (guide - anchor) / vector;
    if (minimum..=maximum).contains(&candidate) {
      let distance = (guide - handle).abs();
      if distance <= threshold
        && (!best.found
          || distance * pane_width < best.distance
          || (distance * pane_width == best.distance && best.object))
      {
        best = SnapGuide {
          found: true,
          object: false,
          adjustment: candidate,
          distance: distance * pane_width,
          guide,
        };
      }
    }
  }
  for &(target_layer, origin, target_extent) in targets {
    if target_layer == layer_id {
      continue;
    }
    for guide in [origin, origin + target_extent / 2.0, origin + target_extent] {
      let candidate = (guide - anchor) / vector;
      if (minimum..=maximum).contains(&candidate) {
        let distance = (guide - handle).abs();
        if distance <= threshold && (!best.found || distance * pane_width <= best.distance) {
          best = SnapGuide {
            found: true,
            object: true,
            adjustment: candidate,
            distance: distance * pane_width,
            guide,
          };
        }
      }
    }
  }
  best
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn move_snaps_to_canvas_inset() {
    let result = move_axis(0.018, 0.2, 1000.0, 600.0, 1.0, &[], 1);
    assert!(result.found && !result.object);
    assert!((result.adjustment + 0.006).abs() < 1e-9);
    assert!((result.guide - 0.012).abs() < 1e-9);
  }

  #[test]
  fn object_snap_wins_equal_distance() {
    let result = move_axis(0.49, 0.1, 1000.0, 600.0, 1.0, &[(2, 0.59, 0.1)], 1);
    assert!(result.found && result.object);
    assert!((result.guide - 0.59).abs() < 1e-9);
  }

  #[test]
  fn resize_returns_uniform_scale_for_target_edge() {
    let result = resize_axis(0.0, 1.0, 0.987, 1000.0, 600.0, 1.0, 0.1, 8.0, &[], 1);
    assert!(result.found);
    assert!((result.adjustment - 0.988).abs() < 1e-9);
  }

  #[test]
  fn resize_object_wins_equal_distance_tie() {
    let result = resize_axis(
      0.0,
      1.0,
      0.496,
      1000.0,
      600.0,
      1.0,
      0.1,
      8.0,
      &[(2, 0.5, 0.1)],
      1,
    );
    assert!(result.found && result.object);
  }
}
