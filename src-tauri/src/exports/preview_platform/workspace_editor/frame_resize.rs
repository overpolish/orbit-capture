// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{
  limits::{
    FRAME_EDGE_BOTTOM, FRAME_EDGE_CENTERED, FRAME_EDGE_LEFT, FRAME_EDGE_RIGHT, FRAME_EDGE_TOP,
    FRAME_MAX_AREA, FRAME_MIN_SIZE,
  },
  FrameId, WorkspaceScene, WorldRect,
};

#[derive(Clone, Debug)]
pub struct FrameResizeResult {
  pub scene: WorkspaceScene,
  pub old_rect: WorldRect,
  pub new_rect: WorldRect,
  /// Integer output dimensions used by the media/render surface.
  pub output_size: (u32, u32),
}

/// Pure frame resize operation used by both native GPU adapters.
pub fn resize_frame(
  source: &WorkspaceScene,
  frame_id: FrameId,
  edges: u32,
  delta: (f64, f64),
) -> Result<FrameResizeResult, String> {
  let old_frame = source.frame(frame_id).ok_or("frame missing")?.clone();
  let horizontal = edges & (FRAME_EDGE_LEFT | FRAME_EDGE_RIGHT) != 0;
  let vertical = edges & (FRAME_EDGE_TOP | FRAME_EDGE_BOTTOM) != 0;
  if !horizontal && !vertical {
    return Err("frame resize has no edges".into());
  }
  if !delta.0.is_finite() || !delta.1.is_finite() {
    return Err("frame resize delta is not finite".into());
  }

  let centered = edges & FRAME_EDGE_CENTERED != 0;
  let left = edges & FRAME_EDGE_LEFT != 0;
  let right = edges & FRAME_EDGE_RIGHT != 0;
  let top = edges & FRAME_EDGE_TOP != 0;
  let bottom = edges & FRAME_EDGE_BOTTOM != 0;
  let (mut x, mut width) = resized_axis(
    old_frame.rect.x,
    old_frame.rect.width,
    delta.0,
    left,
    right,
    centered,
  );
  let (mut y, mut height) = resized_axis(
    old_frame.rect.y,
    old_frame.rect.height,
    delta.1,
    top,
    bottom,
    centered,
  );

  // Frame dimensions correspond to output pixels. Round before applying the
  // safety bounds so Metal and D3D receive identical integer sizes.
  width = width.round().max(FRAME_MIN_SIZE);
  height = height.round().max(FRAME_MIN_SIZE);
  let area = width * height;
  if area > FRAME_MAX_AREA {
    let factor = (FRAME_MAX_AREA / area).sqrt();
    width = (width * factor).floor().max(FRAME_MIN_SIZE);
    height = (height * factor).floor().max(FRAME_MIN_SIZE);
  }
  // Re-anchor the opposite edge (or the center for centered resizing) after
  // integer rounding and max-area limiting.
  x = anchored_origin(
    old_frame.rect.x,
    old_frame.rect.width,
    x,
    width,
    left,
    right,
    centered,
  );
  y = anchored_origin(
    old_frame.rect.y,
    old_frame.rect.height,
    y,
    height,
    top,
    bottom,
    centered,
  );
  let new_rect = WorldRect {
    x,
    y,
    width,
    height,
  };

  let mut next = source.clone();
  let old_rect = old_frame.rect;
  if let Some(frame) = next.frames.iter_mut().find(|frame| frame.id == frame_id) {
    frame.rect = new_rect;
  }
  for layer in &mut next.layers {
    if layer.frame_id != frame_id {
      continue;
    }
    let world = old_rect.normalized(
      layer.rect.x,
      layer.rect.y,
      layer.rect.width,
      layer.rect.height,
    );
    layer.rect = new_rect.to_normalized(world);
  }
  next.revision = source.revision.saturating_add(1);
  next.validate()?;
  Ok(FrameResizeResult {
    scene: next,
    old_rect,
    new_rect,
    output_size: (new_rect.width as u32, new_rect.height as u32),
  })
}

fn resized_axis(
  origin: f64,
  size: f64,
  delta: f64,
  min_edge: bool,
  max_edge: bool,
  centered: bool,
) -> (f64, f64) {
  if centered && (min_edge || max_edge) {
    let amount = if min_edge { -delta } else { delta };
    (origin - amount, size + amount * 2.0)
  } else if min_edge && !max_edge {
    (origin + delta, size - delta)
  } else if max_edge && !min_edge {
    (origin, size + delta)
  } else {
    (origin, size)
  }
}

fn anchored_origin(
  old_origin: f64,
  old_size: f64,
  proposed_origin: f64,
  new_size: f64,
  min_edge: bool,
  max_edge: bool,
  centered: bool,
) -> f64 {
  if centered && (min_edge || max_edge) {
    old_origin + (old_size - new_size) / 2.0
  } else if min_edge && !max_edge {
    old_origin + old_size - new_size
  } else {
    proposed_origin
  }
}
