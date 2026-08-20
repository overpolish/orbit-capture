// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::limits::FRAME_MIN_SIZE;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

impl WorldRect {
  pub fn normalized(self, x: f64, y: f64, width: f64, height: f64) -> Self {
    Self {
      x: self.x + x * self.width,
      y: self.y + y * self.height,
      width: width * self.width,
      height: height * self.height,
    }
  }
  pub fn to_normalized(self, world: WorldRect) -> NormalizedRect {
    NormalizedRect {
      x: (world.x - self.x) / self.width,
      y: (world.y - self.y) / self.height,
      width: world.width / self.width,
      height: world.height / self.height,
    }
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NormalizedRect {
  pub x: f64,
  pub y: f64,
  pub width: f64,
  pub height: f64,
}

/// Grow a canvas to contain every layer crop, then express all layer
/// geometry in the new canvas. Absolute crop/image pixels are preserved.
pub fn fit_canvas_to_layers(
  canvas: (u32, u32),
  layers: &[LayerGeometry],
) -> ((u32, u32), Vec<LayerGeometry>) {
  let width = f64::from(canvas.0.max(1));
  let height = f64::from(canvas.1.max(1));
  let mut left = 0.0_f64;
  let mut top = 0.0_f64;
  let mut right = width;
  let mut bottom = height;
  for layer in layers {
    let crop = layer.crop;
    left = left.min((crop.x * width).floor());
    top = top.min((crop.y * height).floor());
    right = right.max(((crop.x + crop.width) * width).ceil());
    bottom = bottom.max(((crop.y + crop.height) * height).ceil());
  }
  let next_width = (right - left).round().max(FRAME_MIN_SIZE) as u32;
  let next_height = (bottom - top).round().max(FRAME_MIN_SIZE) as u32;
  let next_width_f = f64::from(next_width);
  let next_height_f = f64::from(next_height);
  let layers = layers
    .iter()
    .map(|layer| LayerGeometry {
      crop: NormalizedRect {
        x: (layer.crop.x * width - left) / next_width_f,
        y: (layer.crop.y * height - top) / next_height_f,
        width: layer.crop.width * width / next_width_f,
        height: layer.crop.height * height / next_height_f,
      },
      image_center_x: (layer.image_center_x * width - left) / next_width_f,
      image_center_y: (layer.image_center_y * height - top) / next_height_f,
      image_width: layer.image_width * width / next_width_f,
      radius_percent: layer.radius_percent,
    })
    .collect();
  ((next_width, next_height), layers)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureOperation {
  Move = 0,
  Resize = 1,
  Radius = 2,
}

/// The complete editable geometry of a visual layer, normalized to its frame.
/// Keeping the crop and underlying image transform together prevents pixels
/// and OSCs from being advanced by different gesture equations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerGeometry {
  pub crop: NormalizedRect,
  pub image_center_x: f64,
  pub image_center_y: f64,
  pub image_width: f64,
  pub radius_percent: f64,
}

/// Applies one native pointer update to an immutable layer snapshot. `delta`
/// is the final crop-origin delta already resolved by the platform hit test;
/// `scale` is the final uniform resize factor. This is intentionally the same
/// contract used by the existing Metal and D3D gesture callbacks.
pub fn apply_layer_gesture(
  start: LayerGeometry,
  operation: GestureOperation,
  delta: (f64, f64),
  scale: f64,
) -> LayerGeometry {
  let mut next = start;
  match operation {
    GestureOperation::Move => {
      next.crop.x += delta.0;
      next.crop.y += delta.1;
      next.image_center_x += delta.0;
      next.image_center_y += delta.1;
    }
    GestureOperation::Resize => {
      let scale = scale.clamp(0.0, 8.0);
      let next_x = start.crop.x + delta.0;
      let next_y = start.crop.y + delta.1;
      let transform = |value: f64, start_frame: f64, next_frame: f64| {
        if (scale - 1.0).abs() < 1e-9 {
          value
        } else {
          let anchor = (next_frame - start_frame * scale) / (1.0 - scale);
          anchor + (value - anchor) * scale
        }
      };
      next.crop = NormalizedRect {
        x: next_x,
        y: next_y,
        width: start.crop.width * scale,
        height: start.crop.height * scale,
      };
      next.image_center_x = transform(start.image_center_x, start.crop.x, next_x);
      next.image_center_y = transform(start.image_center_y, start.crop.y, next_y);
      next.image_width = start.image_width * scale;
    }
    GestureOperation::Radius => next.radius_percent = scale.clamp(0.0, 50.0),
  }
  next
}

/// Rebase a normalized layer geometry while preserving its absolute
/// workspace-space crop and image transform.
pub fn rebase_layer_geometry(
  geometry: LayerGeometry,
  old_frame: WorldRect,
  new_frame: WorldRect,
) -> LayerGeometry {
  let crop_world = old_frame.normalized(
    geometry.crop.x,
    geometry.crop.y,
    geometry.crop.width,
    geometry.crop.height,
  );
  let image_center_world =
    old_frame.normalized(geometry.image_center_x, geometry.image_center_y, 0.0, 0.0);
  LayerGeometry {
    crop: new_frame.to_normalized(crop_world),
    image_center_x: (image_center_world.x - new_frame.x) / new_frame.width,
    image_center_y: (image_center_world.y - new_frame.y) / new_frame.height,
    image_width: geometry.image_width * old_frame.width / new_frame.width,
    radius_percent: geometry.radius_percent,
  }
}
