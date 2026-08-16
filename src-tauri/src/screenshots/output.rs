// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[cfg(not(target_os = "macos"))]
use super::{mesh::mesh_canvas, rounded_corners};
use super::{mesh::MeshGradientPoint, CapturedImage};

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

pub(crate) fn parse_hex_colour(value: &str) -> Result<[u8; 4], String> {
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

pub(crate) fn output_dimensions(settings: &ScreenshotOutputSettings) -> Result<(u32, u32), String> {
  if settings.width < 64
    || settings.height < 64
    || u64::from(settings.width) * u64::from(settings.height) > MAX_OUTPUT_PIXELS
  {
    return Err("The screenshot output dimensions are not valid".to_owned());
  }
  Ok((settings.width, settings.height))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OutputPlacement {
  pub crop_height: u32,
  pub crop_width: u32,
  pub crop_x: i32,
  pub crop_y: i32,
  pub image_height: u32,
  pub image_width: u32,
  pub image_x: f64,
  pub image_y: f64,
}

pub(crate) fn output_placement(
  source_width: u32,
  source_height: u32,
  settings: &ScreenshotOutputSettings,
) -> Result<OutputPlacement, String> {
  let (output_width, output_height) = output_dimensions(settings)?;
  let percentages = [
    settings.screenshot_crop_height_percent,
    settings.screenshot_crop_width_percent,
    settings.screenshot_crop_x_percent,
    settings.screenshot_crop_y_percent,
    settings.screenshot_image_width_percent,
    settings.screenshot_image_x_percent,
    settings.screenshot_image_y_percent,
  ];
  if source_width == 0
    || source_height == 0
    || percentages.iter().any(|value| !value.is_finite())
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
  let image_height = (f64::from(image_width) * f64::from(source_height) / f64::from(source_width))
    .round()
    .max(1.0) as u32;
  let mut crop_height = (f64::from(output_height) * settings.screenshot_crop_height_percent / 100.0)
    .round()
    .max(1.0) as u32;
  let mut crop_width = (f64::from(output_width) * settings.screenshot_crop_width_percent / 100.0)
    .round()
    .max(1.0) as u32;
  let mut crop_x =
    (f64::from(output_width) * settings.screenshot_crop_x_percent / 100.0).round() as i32;
  let mut crop_y =
    (f64::from(output_height) * settings.screenshot_crop_y_percent / 100.0).round() as i32;
  let image_x = f64::from(output_width) * settings.screenshot_image_x_percent / 100.0
    - f64::from(image_width) / 2.0;
  let image_y = f64::from(output_height) * settings.screenshot_image_y_percent / 100.0
    - f64::from(image_height) / 2.0;
  // Preview resolution scaling can round a crop and its otherwise coincident
  // placed source to opposite sides of the same pixel. Snap all four edges
  // when they differ by no more than one pixel; a deliberate crop edit moves
  // farther than this and remains independent.
  let image_left = image_x.round() as i32;
  let image_top = image_y.round() as i32;
  let image_right = image_left.saturating_add(image_width as i32);
  let image_bottom = image_top.saturating_add(image_height as i32);
  let crop_right = crop_x.saturating_add(crop_width as i32);
  let crop_bottom = crop_y.saturating_add(crop_height as i32);
  if (crop_x - image_left).abs() <= 1
    && (crop_right - image_right).abs() <= 1
    && (crop_y - image_top).abs() <= 1
    && (crop_bottom - image_bottom).abs() <= 1
  {
    crop_x = image_left;
    crop_y = image_top;
    crop_width = image_width;
    crop_height = image_height;
  }
  Ok(OutputPlacement {
    crop_height,
    crop_width,
    crop_x,
    crop_y,
    image_height,
    image_width,
    image_x,
    image_y,
  })
}

#[cfg(target_os = "macos")]
pub fn compose_screenshot(
  image: &CapturedImage,
  settings: &ScreenshotOutputSettings,
) -> Result<CapturedImage, String> {
  super::platform::compose_output_layers(image, settings, 0.0, true, None, None, None, false, false)
}

#[cfg(not(target_os = "macos"))]
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
  let placement = output_placement(image.width, image.height, settings)?;
  let image_width = placement.image_width;
  let image_height = placement.image_height;
  if u64::from(image_width) * u64::from(image_height) > MAX_OUTPUT_PIXELS * 4 {
    return Err("The scaled screenshot is too large".to_owned());
  }
  let image_x = placement.image_x;
  let image_y = placement.image_y;
  let crop_x = f64::from(placement.crop_x);
  let crop_y = f64::from(placement.crop_y);
  let crop_width = placement.crop_width;
  let crop_height = placement.crop_height;
  let source_x = (crop_x - image_x).round() as i64;
  let source_y = (crop_y - image_y).round() as i64;
  let resized = image::imageops::resize(
    &source,
    image_width,
    image_height,
    image::imageops::FilterType::Lanczos3,
  );
  // Crop and image are independently movable. Their overlap is the video;
  // uncovered crop pixels stay transparent so the configured background
  // shows through, matching the GPU preview shader instead of rejecting a
  // valid artistic placement.
  let mut cropped = image::RgbaImage::new(crop_width, crop_height);
  image::imageops::overlay(&mut cropped, &resized, -source_x, -source_y);
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
  let visible_left = crop_x.max(image_x);
  let visible_top = crop_y.max(image_y);
  let visible_right = (crop_x + f64::from(crop_width)).min(image_x + f64::from(image_width));
  let visible_bottom = (crop_y + f64::from(crop_height)).min(image_y + f64::from(image_height));
  let visible_width = (visible_right - visible_left).max(0.0);
  let visible_height = (visible_bottom - visible_top).max(0.0);
  let shadow_margin = visible_left
    .min(visible_top)
    .min(f64::from(output_width) - visible_right)
    .min(f64::from(output_height) - visible_bottom)
    .max(0.0);
  if settings.drop_shadow
    && visible_width > 0.0
    && visible_height > 0.0
    && shadow_margin * 0.45 > 1.0
  {
    let sigma = (visible_width.min(visible_height) * 0.055)
      .clamp(10.0, 110.0)
      .min(shadow_margin * 0.45) as f32;
    let padding = (sigma * 3.0).ceil() as u32;
    let mut shadow = image::RgbaImage::new(
      crop_width.saturating_add(padding.saturating_mul(2)),
      crop_height.saturating_add(padding.saturating_mul(2)),
    );
    for (x, y, pixel) in foreground.enumerate_pixels() {
      shadow.put_pixel(
        x + padding,
        y + padding,
        image::Rgba([0, 0, 0, ((f32::from(pixel[3]) / 255.0) * 36.0) as u8]),
      );
    }
    let shadow = image::imageops::blur(&shadow, sigma);
    let offset = (sigma * 0.35).round() as i64;
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

  fn assert_colour_close(actual: &[u8], expected: [u8; 4]) {
    assert!(actual
      .iter()
      .zip(expected)
      .all(|(actual, expected)| actual.abs_diff(expected) <= 1));
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
    assert_colour_close(pixel(0, 0), [17, 34, 51, 255]);
    assert_colour_close(pixel(200, 200), [200, 100, 50, 255]);
    assert_colour_close(pixel(200, 100), [17, 34, 51, 255]);
  }

  #[test]
  fn snaps_a_one_pixel_preview_rounding_gap_but_keeps_a_real_crop() {
    let mut output_settings = settings(401, 401);
    let exact = output_placement(200, 100, &output_settings).unwrap();
    output_settings.screenshot_crop_height_percent += 100.0 / 401.0;
    let snapped = output_placement(200, 100, &output_settings).unwrap();
    assert_eq!(snapped.crop_x, exact.image_x.round() as i32);
    assert_eq!(snapped.crop_y, exact.image_y.round() as i32);
    assert_eq!(snapped.crop_width, exact.image_width);
    assert_eq!(snapped.crop_height, exact.image_height);

    output_settings.screenshot_crop_height_percent += 300.0 / 401.0;
    let deliberate = output_placement(200, 100, &output_settings).unwrap();
    assert_ne!(deliberate.crop_height, deliberate.image_height);
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

    assert_colour_close(pixel(0, 200), [200, 100, 50, 255]);
    assert_colour_close(pixel(300, 200), [17, 34, 51, 255]);
  }

  #[test]
  fn allows_the_image_to_cover_only_part_of_its_crop_window() {
    let mut output_settings = settings(400, 400);
    output_settings.screenshot_image_x_percent = 25.0;
    let output = compose_screenshot(
      &solid_image(200, 100, [200, 100, 50, 255]),
      &output_settings,
    )
    .unwrap();
    let pixel = |x: u32, y: u32| {
      let start = ((y * output.width + x) * 4) as usize;
      &output.rgba[start..start + 4]
    };

    assert_colour_close(pixel(100, 200), [200, 100, 50, 255]);
    assert_colour_close(pixel(300, 200), [17, 34, 51, 255]);
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
