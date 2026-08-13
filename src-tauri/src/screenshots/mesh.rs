// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeshGradientPoint {
  pub radius_x: f64,
  pub radius_y: f64,
  pub rotation: f64,
  pub x: f64,
  pub y: f64,
}

fn parse_hex_colour(value: &str) -> Result<[u8; 4], String> {
  let value = value.strip_prefix('#').unwrap_or(value);
  if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err("The screenshot mesh colour is not valid".to_owned());
  }
  let channel = |start| u8::from_str_radix(&value[start..start + 2], 16).map_err(|e| e.to_string());
  Ok([channel(0)?, channel(2)?, channel(4)?, u8::MAX])
}

pub(super) fn mesh_canvas(
  width: u32,
  height: u32,
  colors: &[String],
  points: &[MeshGradientPoint],
  seed: u32,
  warp_percent: f64,
) -> Result<image::RgbaImage, String> {
  validate_mesh(colors, points, warp_percent)?;
  let parsed = colors
    .iter()
    .map(|color| parse_hex_colour(color))
    .collect::<Result<Vec<_>, _>>()?;
  super::mesh_gpu::render(width, height, &parsed, points, seed, warp_percent)
}

pub(crate) fn validate_mesh(
  colors: &[String],
  points: &[MeshGradientPoint],
  warp_percent: f64,
) -> Result<(), String> {
  // One base colour plus up to four control-point colours. More than five
  // becomes visually muddy rather than adding useful variation.
  let invalid = !(3..=4).contains(&points.len())
    || colors.len() != points.len() + 1
    || !warp_percent.is_finite()
    || !(0.0..=20.0).contains(&warp_percent)
    || points.iter().any(|point| {
      !point.x.is_finite()
        || !point.y.is_finite()
        || !point.radius_x.is_finite()
        || !point.radius_y.is_finite()
        || !point.rotation.is_finite()
        || !(-25.0..=125.0).contains(&point.x)
        || !(-25.0..=125.0).contains(&point.y)
        || !(20.0..=120.0).contains(&point.radius_x)
        || !(20.0..=120.0).contains(&point.radius_y)
        || !(-360.0..=360.0).contains(&point.rotation)
    });
  if invalid {
    Err("The screenshot mesh background is not valid".to_owned())
  } else {
    Ok(())
  }
}
