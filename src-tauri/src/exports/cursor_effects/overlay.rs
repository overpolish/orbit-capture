// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const OVERLAY_CACHE_CAPACITY: usize = 12;

#[derive(Clone, Copy, PartialEq)]
struct OverlayVisualKey {
  blur_direction_x: i16,
  blur_direction_y: i16,
  blur_distance: i16,
  height: i32,
  hotspot_x: i32,
  hotspot_y: i32,
  rotation: i16,
  scale: i16,
  style: CursorStyle,
  layer_size: usize,
  width: i32,
}

pub(in crate::exports) struct CursorOverlayCache {
  entries: Vec<(OverlayVisualKey, Vec<u8>)>,
}

impl CursorOverlayCache {
  pub(in crate::exports) fn new() -> Self {
    Self {
      entries: Vec::with_capacity(OVERLAY_CACHE_CAPACITY),
    }
  }

  fn get(&mut self, key: OverlayVisualKey) -> Option<&[u8]> {
    let index = self
      .entries
      .iter()
      .position(|(candidate, _)| *candidate == key)?;
    let entry = self.entries.remove(index);
    self.entries.push(entry);
    self.entries.last().map(|(_, pixels)| pixels.as_slice())
  }

  fn insert(&mut self, key: OverlayVisualKey, pixels: &[u8]) {
    if self.entries.len() == OVERLAY_CACHE_CAPACITY {
      self.entries.remove(0);
    }
    self.entries.push((key, pixels.to_vec()));
  }
}

fn quantize(value: f64, steps: f64) -> i16 {
  (value * steps)
    .round()
    .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

fn visual_key(
  output: OutputCursor,
  settings: CursorEffectSettings,
  layer_size: usize,
) -> OverlayVisualKey {
  let travel = output.delta_x.hypot(output.delta_y);
  let blur_distance = if settings.motion_blur {
    travel.min(MAX_BLUR_DISTANCE)
  } else {
    0.0
  };
  let (blur_direction_x, blur_direction_y) = if blur_distance > 1.25 && travel > 0.0 {
    (output.delta_x / travel, output.delta_y / travel)
  } else {
    (0.0, 0.0)
  };
  OverlayVisualKey {
    blur_direction_x: quantize(blur_direction_x, 16.0),
    blur_direction_y: quantize(blur_direction_y, 16.0),
    blur_distance: quantize(blur_distance, 0.25),
    height: (output.height * 100.0).round() as i32,
    hotspot_x: (output.hotspot_x * 100.0).round() as i32,
    hotspot_y: (output.hotspot_y * 100.0).round() as i32,
    rotation: quantize(output.cursor.rotation_degrees, 1.0),
    scale: quantize(output.cursor.scale, 50.0),
    style: output.cursor.appearance.style,
    layer_size,
    width: (output.width * 100.0).round() as i32,
  }
}

impl CursorCompositor {
  pub(in crate::exports) fn overlay_size(
    &self,
    output_width: usize,
    output_height: usize,
    settings: CursorEffectSettings,
  ) -> usize {
    let scale = settings.size_percent.clamp(50.0, 500.0) / 100.0;
    let radius = self
      .appearances
      .iter()
      .map(|appearance| {
        let width = appearance.width / self.source.width * output_width as f64;
        let height = appearance.height / self.source.height * output_height as f64;
        width.hypot(height) * scale
      })
      .fold(0.0, f64::max);
    let blur = if settings.motion_blur {
      MAX_BLUR_DISTANCE
    } else {
      0.0
    };
    ((radius + blur + 4.0).ceil() as usize * 2)
      .max(16)
      .next_multiple_of(2)
  }

  pub(in crate::exports) fn composite_overlay_bgra(
    &self,
    pixels: &mut [u8],
    layer_size: usize,
    output_size: (usize, usize),
    position_ms: u64,
    settings: CursorEffectSettings,
    cache: &mut CursorOverlayCache,
  ) -> Option<CursorOverlayPosition> {
    let output = self.output_cursor(position_ms, output_size.0, output_size.1, settings)?;
    let half = layer_size as f64 / 2.0;
    let left = (output.x - half).floor();
    let top = (output.y - half).floor();
    let key = visual_key(output, settings, layer_size);
    if let Some(cached) = cache.get(key) {
      pixels.copy_from_slice(cached);
    } else {
      pixels.fill(0);
      let mut frame = raster::FrameMut {
        height: layer_size,
        pixels,
        stride: layer_size * 4,
        width: layer_size,
      };
      self.draw_output(
        &mut frame,
        output,
        output.x - left,
        output.y - top,
        settings,
      );
      cache.insert(key, pixels);
    }
    Some(CursorOverlayPosition {
      x: left as i32,
      y: top as i32,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::{CursorOverlayCache, OverlayVisualKey};
  use crate::exports::cursor_effects::CursorStyle;

  fn key(layer_size: usize) -> OverlayVisualKey {
    OverlayVisualKey {
      blur_direction_x: 0,
      blur_direction_y: 0,
      blur_distance: 0,
      height: 32,
      hotspot_x: 1,
      hotspot_y: 1,
      rotation: 0,
      scale: 50,
      style: CursorStyle::Arrow,
      layer_size,
      width: 24,
    }
  }

  #[test]
  fn cache_does_not_reuse_pixels_with_a_different_layer_size() {
    let mut cache = CursorOverlayCache::new();
    cache.insert(key(16), &[1; 16 * 16 * 4]);
    cache.insert(key(18), &[2; 18 * 18 * 4]);

    assert_eq!(
      cache.get(key(16)).map(|pixels| pixels.len()),
      Some(16 * 16 * 4)
    );
    assert_eq!(
      cache.get(key(18)).map(|pixels| pixels.len()),
      Some(18 * 18 * 4)
    );
  }
}
