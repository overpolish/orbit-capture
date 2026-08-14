// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::recording::cursor::CursorStyle;
use image::RgbaImage;
use rayon::prelude::*;

#[cfg(target_os = "macos")]
#[path = "raster/platform_macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "raster/platform_unsupported.rs"]
mod platform;

mod fallback;

pub(super) fn uses_same_artwork(left: CursorStyle, right: CursorStyle) -> bool {
  platform::canonical_style(left) == platform::canonical_style(right)
}

pub(super) fn initialize_system_artwork() {
  platform::initialize();
}

#[derive(Clone, Copy)]
pub(super) struct CursorRaster {
  artwork: fallback::Artwork,
  cos: f64,
  height: f64,
  hotspot_x: f64,
  hotspot_y: f64,
  scale: f64,
  sin: f64,
  system_artwork: Option<&'static RgbaImage>,
  width: f64,
}

pub(super) struct FrameMut<'a> {
  pub height: usize,
  pub pixels: &'a mut [u8],
  pub stride: usize,
  pub width: usize,
}

impl CursorRaster {
  #[allow(clippy::too_many_arguments)]
  pub(super) fn new(
    style: CursorStyle,
    rotation_degrees: f64,
    width: f64,
    height: f64,
    hotspot_x: f64,
    hotspot_y: f64,
    scale: f64,
  ) -> Self {
    let system_artwork = platform::artwork(style);
    let vertical = system_artwork.is_none() && fallback::is_vertical(style);
    #[cfg(target_os = "windows")]
    let rotation_degrees = -rotation_degrees;
    let rotation = rotation_degrees.to_radians()
      + if vertical {
        std::f64::consts::FRAC_PI_2
      } else {
        0.0
      };
    let (sin, cos) = rotation.sin_cos();
    Self {
      artwork: fallback::artwork(style),
      cos,
      height,
      hotspot_x,
      hotspot_y,
      scale,
      sin,
      system_artwork,
      width,
    }
  }

  fn sample(self, destination_x: f64, destination_y: f64, x: f64, y: f64) -> [f64; 4] {
    let dx = destination_x - x;
    let dy = destination_y - y;
    let local_x = (self.cos * dx + self.sin * dy) / self.scale + self.hotspot_x;
    let local_y = (-self.sin * dx + self.cos * dy) / self.scale + self.hotspot_y;
    let fallback_arrow = self.system_artwork.is_none() && self.artwork == fallback::Artwork::Arrow;
    if !fallback_arrow
      && (!(0.0..self.width).contains(&local_x) || !(0.0..self.height).contains(&local_y))
    {
      return [0.0; 4];
    }
    self.system_artwork.map_or_else(
      || {
        let design_size = if self.artwork == fallback::Artwork::Hand {
          (32.0, 32.0)
        } else {
          (28.0, 40.0)
        };
        let artwork_scale = (self.width / design_size.0)
          .min(self.height / design_size.1)
          .max(0.01);
        let (origin_x, origin_y) = fallback::origin(self.artwork);
        let design_x = local_x / artwork_scale + origin_x;
        let design_y = local_y / artwork_scale + origin_y;
        if fallback_arrow && (!(0.0..28.0).contains(&design_x) || !(0.0..40.0).contains(&design_y))
        {
          return [0.0; 4];
        }
        fallback::sample(self.artwork, design_x, design_y)
      },
      |artwork| {
        sample_image(
          artwork,
          local_x / self.width * f64::from(artwork.width()),
          local_y / self.height * f64::from(artwork.height()),
        )
      },
    )
  }

  fn sample_for_draw(self, destination_x: f64, destination_y: f64, x: f64, y: f64) -> [f64; 4] {
    if self.system_artwork.is_some() {
      return self.sample(destination_x, destination_y, x, y);
    }

    const OFFSETS: [f64; 4] = [-0.375, -0.125, 0.125, 0.375];
    let mut alpha = 0.0;
    let mut color = [0.0; 3];
    for offset_y in OFFSETS {
      for offset_x in OFFSETS {
        let source = self.sample(destination_x + offset_x, destination_y + offset_y, x, y);
        let sample_alpha = source[3] / 255.0;
        alpha += sample_alpha;
        for channel in 0..3 {
          color[channel] += source[channel] * sample_alpha;
        }
      }
    }
    if alpha <= 0.0 {
      return [0.0; 4];
    }
    for channel in &mut color {
      *channel /= alpha;
    }
    [color[0], color[1], color[2], alpha / 16.0 * 255.0]
  }

  fn radius(self) -> f64 {
    self.width.hypot(self.height) * self.scale
  }
}

fn sample_image(image: &RgbaImage, x: f64, y: f64) -> [f64; 4] {
  let x = x.clamp(0.0, f64::from(image.width().saturating_sub(1)));
  let y = y.clamp(0.0, f64::from(image.height().saturating_sub(1)));
  let x0 = x.floor() as u32;
  let y0 = y.floor() as u32;
  let x1 = (x0 + 1).min(image.width() - 1);
  let y1 = (y0 + 1).min(image.height() - 1);
  let fraction_x = x - f64::from(x0);
  let fraction_y = y - f64::from(y0);
  let samples = [
    (
      image.get_pixel(x0, y0).0,
      (1.0 - fraction_x) * (1.0 - fraction_y),
    ),
    (image.get_pixel(x1, y0).0, fraction_x * (1.0 - fraction_y)),
    (image.get_pixel(x0, y1).0, (1.0 - fraction_x) * fraction_y),
    (image.get_pixel(x1, y1).0, fraction_x * fraction_y),
  ];
  let mut alpha = 0.0;
  let mut color = [0.0; 3];
  for (sample, weight) in samples {
    let sample_alpha = f64::from(sample[3]) / 255.0;
    alpha += sample_alpha * weight;
    for channel in 0..3 {
      color[channel] += f64::from(sample[channel]) * sample_alpha * weight;
    }
  }
  if alpha <= 0.0 {
    return [0.0; 4];
  }
  [
    color[0] / alpha,
    color[1] / alpha,
    color[2] / alpha,
    alpha * 255.0,
  ]
}

fn blend(frame: &mut FrameMut<'_>, x: usize, y: usize, source: [f64; 4]) {
  let offset = y * frame.stride;
  let Some(row) = frame.pixels.get_mut(offset..offset + frame.stride) else {
    return;
  };
  blend_row(row, x, source);
}

fn blend_row(row: &mut [u8], x: usize, source: [f64; 4]) {
  let source_alpha = source[3].clamp(0.0, 1.0);
  if source_alpha <= 0.0 {
    return;
  }
  let offset = x * 4;
  if offset + 3 >= row.len() {
    return;
  }
  let destination_alpha = f64::from(row[offset + 3]) / 255.0;
  let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
  for (destination, source) in row[offset..offset + 3]
    .iter_mut()
    .zip([source[2], source[1], source[0]])
  {
    let premultiplied =
      source * source_alpha + f64::from(*destination) * destination_alpha * (1.0 - source_alpha);
    *destination = (premultiplied / output_alpha).round() as u8;
  }
  row[offset + 3] = (output_alpha * 255.0).round() as u8;
}

pub(super) fn draw(frame: &mut FrameMut<'_>, cursor: CursorRaster, x: f64, y: f64) {
  let radius = cursor.radius();
  let bounds = bounds(frame, x, y, radius, 0.0);
  for destination_y in bounds.1..bounds.3 {
    for destination_x in bounds.0..bounds.2 {
      // The native cursor artwork already carries an antialiased alpha edge,
      // and `sample_image` interpolates it while scaling and rotating. A 4x4
      // supersample here did the same texture lookup sixteen times per output
      // pixel, which made a large cursor layer slower than the recording it
      // was being composited onto without improving its edge.
      let mut source =
        cursor.sample_for_draw(destination_x as f64 + 0.5, destination_y as f64 + 0.5, x, y);
      source[3] /= 255.0;
      blend(frame, destination_x, destination_y, source);
    }
  }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_blurred(
  frame: &mut FrameMut<'_>,
  cursor: CursorRaster,
  x: f64,
  y: f64,
  direction_x: f64,
  direction_y: f64,
  distance: f64,
  sample_count: usize,
) {
  let bounds = bounds(frame, x, y, cursor.radius(), distance);
  let weights = (0..sample_count)
    .map(|index| {
      let progress = index as f64 / (sample_count - 1) as f64;
      let centered = (progress - 0.5) / 0.34;
      (-0.5 * centered * centered).exp()
    })
    .collect::<Vec<_>>();
  let total_weight = weights.iter().sum::<f64>();
  frame
    .pixels
    .par_chunks_mut(frame.stride)
    .enumerate()
    .skip(bounds.1)
    .take(bounds.3 - bounds.1)
    .for_each(|(destination_y, row)| {
      for destination_x in bounds.0..bounds.2 {
        let mut alpha = 0.0;
        let mut color = [0.0; 3];
        for (index, weight) in weights.iter().enumerate() {
          let progress = index as f64 / (sample_count - 1) as f64;
          let exposure_offset = (progress - 0.8) * distance;
          let source = cursor.sample(
            destination_x as f64 + 0.5,
            destination_y as f64 + 0.5,
            x + direction_x * exposure_offset,
            y + direction_y * exposure_offset,
          );
          let sample_alpha = source[3] / 255.0;
          alpha += sample_alpha * weight;
          for channel in 0..3 {
            color[channel] += source[channel] * sample_alpha * weight;
          }
        }
        alpha /= total_weight;
        if alpha > 0.0 {
          for channel in &mut color {
            *channel /= total_weight * alpha;
          }
          blend_row(row, destination_x, [color[0], color[1], color[2], alpha]);
        }
      }
    });
}

fn bounds(
  frame: &FrameMut<'_>,
  x: f64,
  y: f64,
  radius: f64,
  blur: f64,
) -> (usize, usize, usize, usize) {
  (
    (x - radius - blur).floor().max(0.0) as usize,
    (y - radius - blur).floor().max(0.0) as usize,
    (x + radius + blur).ceil().min(frame.width as f64) as usize,
    (y + radius + blur).ceil().min(frame.height as f64) as usize,
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn interpolated_artwork_produces_fractional_edge_coverage() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 17.0, 28.0, 40.0, 0.0, 0.0, 4.0);
    let has_partial_pixel = (0..200).any(|y| {
      (0..200).any(|x| {
        let alpha = cursor.sample_for_draw(x as f64 + 0.5, y as f64 + 0.5, 40.0, 40.0)[3];
        alpha > 0.0 && alpha < 255.0
      })
    });
    assert!(has_partial_pixel);
  }

  #[test]
  fn fallback_arrow_keeps_its_native_aspect_inside_a_square_cursor_box() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    let rightmost = (0..32)
      .flat_map(|y| (0..32).map(move |x| (x, y)))
      .filter(|(x, y)| cursor.sample(*x as f64 + 0.5, *y as f64 + 0.5, 0.0, 0.0)[3] > 0.0)
      .map(|(x, _)| x)
      .max()
      .unwrap();

    assert!(
      rightmost <= 23,
      "the 28:40 arrow was stretched to x={rightmost}"
    );
  }

  #[test]
  fn fallback_arrow_places_its_visible_tip_at_the_recorded_hotspot() {
    let cursor = CursorRaster::new(CursorStyle::Arrow, 0.0, 32.0, 32.0, 0.0, 0.0, 1.0);
    assert!(cursor.sample(0.0, 0.0, 0.0, 0.0)[3] > 0.0);
    assert!(
      cursor.sample(-0.5, 0.0, 0.0, 0.0)[3] > 0.0,
      "the rounded tip stroke was clipped at the hotspot"
    );
  }
}
