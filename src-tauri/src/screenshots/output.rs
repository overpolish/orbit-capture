// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

use super::{
  mesh::{mesh_canvas, MeshGradientPoint},
  rounded_corners, CapturedImage,
};

const MAX_OUTPUT_PIXELS: u64 = 120_000_000;

const fn default_hundred() -> f64 {
  100.0
}
const fn default_fifty() -> f64 {
  50.0
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotOutputSettings {
  pub background_color: String,
  pub background_type: String,
  pub background_radius_percent: f64,
  pub drop_shadow: bool,
  pub height: u32,
  #[serde(default, rename = "mode", skip_serializing)]
  pub legacy_mode: Option<String>,
  pub mesh_colors: Vec<String>,
  #[serde(default)]
  pub mesh_locked_colors: Vec<bool>,
  pub mesh_points: Vec<MeshGradientPoint>,
  pub mesh_seed: u32,
  pub mesh_warp_percent: f64,
  pub radius_percent: f64,
  #[serde(default = "default_hundred")]
  pub screenshot_crop_height_percent: f64,
  #[serde(default = "default_hundred")]
  pub screenshot_crop_width_percent: f64,
  #[serde(default)]
  pub screenshot_crop_x_percent: f64,
  #[serde(default)]
  pub screenshot_crop_y_percent: f64,
  #[serde(default = "default_hundred")]
  pub screenshot_image_width_percent: f64,
  #[serde(default = "default_fifty")]
  pub screenshot_image_x_percent: f64,
  #[serde(default = "default_fifty")]
  pub screenshot_image_y_percent: f64,
  pub width: u32,
}

fn parse_hex_colour(value: &str) -> Result<[u8; 4], String> {
  let value = value.strip_prefix('#').unwrap_or(value);
  if !matches!(value.len(), 2 | 3 | 6) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err("The screenshot background colour is not valid".to_owned());
  }
  let expanded = match value.len() {
    2 => value.repeat(3),
    3 => value.chars().flat_map(|character| [character; 2]).collect(),
    _ => value.to_owned(),
  };
  let channel =
    |start| u8::from_str_radix(&expanded[start..start + 2], 16).map_err(|e| e.to_string());
  Ok([channel(0)?, channel(2)?, channel(4)?, u8::MAX])
}

fn output_dimensions(settings: &ScreenshotOutputSettings) -> Result<(u32, u32), String> {
  if settings.width < 64
    || settings.height < 64
    || u64::from(settings.width) * u64::from(settings.height) > MAX_OUTPUT_PIXELS
  {
    return Err("The screenshot output dimensions are not valid".to_owned());
  }
  Ok((settings.width, settings.height))
}

pub fn compose_screenshot(
  image: &CapturedImage,
  settings: &ScreenshotOutputSettings,
) -> Result<CapturedImage, String> {
  let (output_width, output_height) = output_dimensions(settings)?;
  let source = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
    .ok_or_else(|| "The screenshot pixels are not valid".to_owned())?;
  if !settings.radius_percent.is_finite()
    || !(0.0..=50.0).contains(&settings.radius_percent)
    || !settings.background_radius_percent.is_finite()
    || !(0.0..=50.0).contains(&settings.background_radius_percent)
  {
    return Err("The screenshot canvas settings are not valid".to_owned());
  }
  let percentages = [
    settings.screenshot_crop_height_percent,
    settings.screenshot_crop_width_percent,
    settings.screenshot_crop_x_percent,
    settings.screenshot_crop_y_percent,
    settings.screenshot_image_width_percent,
    settings.screenshot_image_x_percent,
    settings.screenshot_image_y_percent,
  ];
  if percentages.iter().any(|value| !value.is_finite())
    || !(1.0..=800.0).contains(&settings.screenshot_crop_width_percent)
    || !(1.0..=800.0).contains(&settings.screenshot_crop_height_percent)
    || settings.screenshot_crop_x_percent.abs() > 800.0
    || settings.screenshot_crop_y_percent.abs() > 800.0
    || !(1.0..=800.0).contains(&settings.screenshot_image_width_percent)
  {
    return Err("The screenshot placement is not valid".to_owned());
  }
  let image_width = (f64::from(output_width) * settings.screenshot_image_width_percent / 100.0)
    .round()
    .max(1.0) as u32;
  let image_height = (f64::from(image_width) * f64::from(image.height) / f64::from(image.width))
    .round()
    .max(1.0) as u32;
  if u64::from(image_width) * u64::from(image_height) > MAX_OUTPUT_PIXELS * 4 {
    return Err("The scaled screenshot is too large".to_owned());
  }
  let image_x = f64::from(output_width) * settings.screenshot_image_x_percent / 100.0
    - f64::from(image_width) / 2.0;
  let image_y = f64::from(output_height) * settings.screenshot_image_y_percent / 100.0
    - f64::from(image_height) / 2.0;
  let crop_x = f64::from(output_width) * settings.screenshot_crop_x_percent / 100.0;
  let crop_y = f64::from(output_height) * settings.screenshot_crop_y_percent / 100.0;
  let crop_width =
    (f64::from(output_width) * settings.screenshot_crop_width_percent / 100.0).round() as u32;
  let crop_height =
    (f64::from(output_height) * settings.screenshot_crop_height_percent / 100.0).round() as u32;
  let source_x = (crop_x - image_x).round() as i64;
  let source_y = (crop_y - image_y).round() as i64;
  if source_x < 0
    || source_y < 0
    || source_x as u64 + u64::from(crop_width) > u64::from(image_width)
    || source_y as u64 + u64::from(crop_height) > u64::from(image_height)
  {
    return Err("The screenshot image no longer covers its crop window".to_owned());
  }
  let resized = image::imageops::resize(
    &source,
    image_width,
    image_height,
    image::imageops::FilterType::Lanczos3,
  );
  let cropped = image::imageops::crop_imm(
    &resized,
    source_x as u32,
    source_y as u32,
    crop_width,
    crop_height,
  )
  .to_image();
  let rounded = rounded_corners(
    &CapturedImage {
      height: crop_height,
      rgba: cropped.into_raw(),
      width: crop_width,
    },
    settings.radius_percent,
  );
  let mut canvas = match settings.background_type.as_str() {
    "mesh" => mesh_canvas(
      output_width,
      output_height,
      &settings.mesh_colors,
      &settings.mesh_points,
      settings.mesh_seed,
      settings.mesh_warp_percent,
    )?,
    "solid" => image::RgbaImage::from_pixel(
      output_width,
      output_height,
      image::Rgba(parse_hex_colour(&settings.background_color)?),
    ),
    _ => return Err("The screenshot background type is not valid".to_owned()),
  };
  let foreground = image::RgbaImage::from_raw(crop_width, crop_height, rounded.rgba)
    .ok_or_else(|| "The screenshot pixels are not valid".to_owned())?;
  let placement_x = crop_x.round() as i64;
  let placement_y = crop_y.round() as i64;
  if settings.drop_shadow {
    let sigma = (f64::from(crop_width.min(crop_height)) * 0.018).clamp(6.0, 48.0) as f32;
    let padding = (sigma * 3.0).ceil() as u32;
    let mut shadow = image::RgbaImage::new(
      crop_width.saturating_add(padding.saturating_mul(2)),
      crop_height.saturating_add(padding.saturating_mul(2)),
    );
    for (x, y, pixel) in foreground.enumerate_pixels() {
      shadow.put_pixel(
        x + padding,
        y + padding,
        image::Rgba([0, 0, 0, ((f32::from(pixel[3]) / 255.0) * 90.0) as u8]),
      );
    }
    let shadow = image::imageops::blur(&shadow, sigma);
    let offset = (sigma * 0.6).round() as i64;
    image::imageops::overlay(
      &mut canvas,
      &shadow,
      placement_x.saturating_sub(i64::from(padding)),
      placement_y
        .saturating_sub(i64::from(padding))
        .saturating_add(offset),
    );
  }
  image::imageops::overlay(&mut canvas, &foreground, placement_x, placement_y);
  Ok(rounded_corners(
    &CapturedImage {
      height: output_height,
      rgba: canvas.into_raw(),
      width: output_width,
    },
    settings.background_radius_percent,
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn solid_image(width: u32, height: u32, colour: [u8; 4]) -> CapturedImage {
    CapturedImage {
      height,
      rgba: colour.repeat((width * height) as usize),
      width,
    }
  }

  fn settings(width: u32, height: u32) -> ScreenshotOutputSettings {
    let placed_width_percent = 80.0;
    let placed_height_percent =
      f64::from(width) * placed_width_percent / 100.0 / 2.0 / f64::from(height) * 100.0;
    ScreenshotOutputSettings {
      background_color: "#112233".to_owned(),
      background_type: "solid".to_owned(),
      background_radius_percent: 0.0,
      drop_shadow: false,
      height,
      legacy_mode: None,
      mesh_colors: vec![
        "#FF0000".to_owned(),
        "#00FF00".to_owned(),
        "#0000FF".to_owned(),
        "#FFFFFF".to_owned(),
        "#000000".to_owned(),
      ],
      mesh_locked_colors: vec![false; 5],
      mesh_points: vec![
        MeshGradientPoint {
          radius_x: 70.0,
          radius_y: 50.0,
          rotation: 20.0,
          x: 15.0,
          y: 15.0,
        },
        MeshGradientPoint {
          radius_x: 45.0,
          radius_y: 70.0,
          rotation: -30.0,
          x: 85.0,
          y: 15.0,
        },
        MeshGradientPoint {
          radius_x: 70.0,
          radius_y: 60.0,
          rotation: 80.0,
          x: 15.0,
          y: 85.0,
        },
        MeshGradientPoint {
          radius_x: 50.0,
          radius_y: 70.0,
          rotation: 0.0,
          x: 85.0,
          y: 85.0,
        },
      ],
      mesh_seed: 42,
      mesh_warp_percent: 9.0,
      radius_percent: 0.0,
      screenshot_crop_height_percent: placed_height_percent,
      screenshot_crop_width_percent: placed_width_percent,
      screenshot_crop_x_percent: 10.0,
      screenshot_crop_y_percent: (100.0 - placed_height_percent) / 2.0,
      screenshot_image_width_percent: placed_width_percent,
      screenshot_image_x_percent: 50.0,
      screenshot_image_y_percent: 50.0,
      width,
    }
  }

  #[test]
  fn uses_the_explicit_output_dimensions() {
    let output = compose_screenshot(
      &solid_image(256, 128, [200, 100, 50, 255]),
      &settings(128, 64),
    )
    .unwrap();

    assert_eq!((output.width, output.height), (128, 64));
  }

  #[test]
  fn fits_a_screenshot_inside_a_custom_coloured_canvas() {
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &settings(400, 400),
    )
    .unwrap();
    let pixel = |x: u32, y: u32| {
      let start = ((y * output.width + x) * 4) as usize;
      &output.rgba[start..start + 4]
    };

    assert_eq!((output.width, output.height), (400, 400));
    assert_eq!(pixel(0, 0), &[17, 34, 51, 255]);
    assert_eq!(pixel(200, 200), &[200, 100, 50, 255]);
    assert_eq!(pixel(200, 100), &[17, 34, 51, 255]);
  }

  #[test]
  fn clips_an_artistically_placed_screenshot_at_the_canvas_edge() {
    let mut output_settings = settings(400, 400);
    output_settings.screenshot_crop_x_percent = -20.0;
    output_settings.screenshot_image_x_percent = 20.0;
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &output_settings,
    )
    .unwrap();
    let pixel = |x: u32, y: u32| {
      let start = ((y * output.width + x) * 4) as usize;
      &output.rgba[start..start + 4]
    };

    assert_eq!(pixel(0, 200), &[200, 100, 50, 255]);
    assert_eq!(pixel(300, 200), &[17, 34, 51, 255]);
  }

  #[test]
  fn accepts_short_hex_background_colours() {
    assert_eq!(parse_hex_colour("#12").unwrap(), [18, 18, 18, 255]);
    assert_eq!(parse_hex_colour("#123").unwrap(), [17, 34, 51, 255]);
  }

  #[test]
  fn renders_a_mesh_background_with_antibanding_grain() {
    let mut output_settings = settings(400, 400);
    output_settings.background_type = "mesh".to_owned();
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &output_settings,
    )
    .unwrap();

    let corner = &output.rgba[..4];
    let opposite_corner = ((399 * output.width + 399) * 4) as usize;
    assert_ne!(corner, &output.rgba[opposite_corner..opposite_corner + 4]);
    assert!(
      (1..64).any(|x| output.rgba[(x * 4)..(x * 4 + 4)] != output.rgba[..4]),
      "the anti-banding tile should contain sub-pixel colour variation"
    );
  }

  #[test]
  fn rounds_the_custom_canvas_background() {
    let mut output_settings = settings(400, 400);
    output_settings.background_radius_percent = 10.0;
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &output_settings,
    )
    .unwrap();

    assert_eq!(output.rgba[3], 0);
    let centre = ((200 * output.width + 200) * 4 + 3) as usize;
    assert_eq!(output.rgba[centre], 255);
  }

  #[test]
  fn adds_the_default_shadow_behind_the_placed_screenshot() {
    let mut output_settings = settings(400, 400);
    output_settings.drop_shadow = true;
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &output_settings,
    )
    .unwrap();
    let start = ((286 * output.width + 200) * 4) as usize;

    assert_ne!(&output.rgba[start..start + 4], &[17, 34, 51, 255]);
  }
}
