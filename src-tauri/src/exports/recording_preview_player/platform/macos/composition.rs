// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::frame::CursorPreview;
use crate::{
  exports::{media_preview, CameraOverlaySettings},
  screenshots::{self, CapturedImage, ScreenshotOutputSettings, StillOverlay},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn composed_layers_rgba(
  screen: &CapturedImage,
  output: &ScreenshotOutputSettings,
  position_ms: u64,
  cursor: Option<(&CursorPreview, CapturedImage)>,
  camera: Option<&CapturedImage>,
  camera_overlay: Option<CameraOverlaySettings>,
  camera_drop_shadow: bool,
  clip_cursor_at_video_edge: bool,
) -> Result<CapturedImage, String> {
  let cursor_image = cursor.as_ref().map(|(_, image)| image);
  let placement = screenshots::output_placement(screen.width, screen.height, output)?;
  let mapped_cursor = cursor.as_ref().map(|(cursor, image)| {
    let scale_x = f64::from(placement.image_width) / f64::from(cursor.canvas_width.max(1));
    let scale_y = f64::from(placement.image_height) / f64::from(cursor.canvas_height.max(1));
    (
      (placement.image_x + f64::from(cursor.x) * scale_x).round() as i32,
      (placement.image_y + f64::from(cursor.y) * scale_y).round() as i32,
      (f64::from(image.width) * scale_x).round().max(1.0) as u32,
      (f64::from(image.height) * scale_y).round().max(1.0) as u32,
    )
  });
  let overlay = camera_overlay
    .map(|settings| {
      camera_still_overlay(
        camera,
        output,
        settings,
        mapped_cursor,
        cursor_image,
        camera_drop_shadow,
      )
    })
    .transpose()?
    .or_else(|| cursor_still_overlay(cursor.as_ref(), mapped_cursor));
  screenshots::compose_output_layers(
    screen,
    output,
    position_ms as f64 / 1_000.0,
    true,
    cursor_image,
    camera,
    overlay.as_ref(),
    clip_cursor_at_video_edge,
  )
}

pub(super) fn still_overlay(
  screen: &CapturedImage,
  output: &ScreenshotOutputSettings,
  cursor: Option<(&CursorPreview, CapturedImage)>,
  camera: Option<&CapturedImage>,
  camera_overlay: Option<CameraOverlaySettings>,
  camera_drop_shadow: bool,
) -> Result<(Option<CapturedImage>, Option<StillOverlay>), String> {
  let cursor_image = cursor.as_ref().map(|(_, image)| image.clone());
  let placement = screenshots::output_placement(screen.width, screen.height, output)?;
  let mapped_cursor = cursor.as_ref().map(|(cursor, image)| {
    let scale_x = f64::from(placement.image_width) / f64::from(cursor.canvas_width.max(1));
    let scale_y = f64::from(placement.image_height) / f64::from(cursor.canvas_height.max(1));
    (
      (placement.image_x + f64::from(cursor.x) * scale_x).round() as i32,
      (placement.image_y + f64::from(cursor.y) * scale_y).round() as i32,
      (f64::from(image.width) * scale_x).round().max(1.0) as u32,
      (f64::from(image.height) * scale_y).round().max(1.0) as u32,
    )
  });
  let overlay = camera_overlay
    .map(|settings| {
      camera_still_overlay(
        camera,
        output,
        settings,
        mapped_cursor,
        cursor_image.as_ref(),
        camera_drop_shadow,
      )
    })
    .transpose()?
    .or_else(|| cursor_still_overlay(cursor.as_ref(), mapped_cursor));
  Ok((cursor_image, overlay))
}

pub(super) fn encoded_jpeg(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let rgba = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
    .ok_or_else(|| "The native preview compositor returned invalid pixels".to_owned())?;
  let mut bytes = Vec::new();
  image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
    .encode_image(&rgba)
    .map_err(|error| error.to_string())?;
  Ok(bytes)
}

pub(super) fn camera_still_overlay(
  camera: Option<&CapturedImage>,
  output: &ScreenshotOutputSettings,
  settings: CameraOverlaySettings,
  cursor: Option<(i32, i32, u32, u32)>,
  cursor_image: Option<&CapturedImage>,
  camera_drop_shadow: bool,
) -> Result<StillOverlay, String> {
  let camera = camera.ok_or_else(|| "Camera pixels are missing from the preview".to_owned())?;
  let geometry = media_preview::bake_geometry(media_preview::BakedVideoExportOptions {
    camera_height: camera.height,
    camera_width: camera.width,
    overlay: settings,
    screen_height: output.height,
    screen_width: output.width,
    video: media_preview::VideoExportOptions {
      compression: 0,
      resolution_scale_percent: 100,
      source_scale_percent: 100,
    },
  })?;
  Ok(StillOverlay {
    cursor_x: cursor.map_or(0, |value| value.0),
    cursor_y: cursor.map_or(0, |value| value.1),
    cursor_width: cursor.map_or(0, |value| value.2),
    cursor_height: cursor.map_or(0, |value| value.3),
    cursor_source_width: cursor_image.map_or(0, |image| image.width),
    cursor_source_height: cursor_image.map_or(0, |image| image.height),
    camera_crop_x: geometry.crop_x,
    camera_crop_y: geometry.crop_y,
    camera_crop_width: geometry.crop_width,
    camera_crop_height: geometry.crop_height,
    camera_frame_x: geometry.frame_x,
    camera_frame_y: geometry.frame_y,
    camera_frame_width: geometry.frame_width,
    camera_frame_height: geometry.frame_height,
    camera_radius: geometry.radius,
    camera_source_width: camera.width,
    camera_source_height: camera.height,
    camera_drop_shadow: u32::from(camera_drop_shadow),
  })
}

pub(super) fn cursor_still_overlay(
  cursor: Option<&(&CursorPreview, CapturedImage)>,
  mapped: Option<(i32, i32, u32, u32)>,
) -> Option<StillOverlay> {
  cursor.map(|(cursor, image)| StillOverlay {
    cursor_x: mapped.map_or(cursor.x, |value| value.0),
    cursor_y: mapped.map_or(cursor.y, |value| value.1),
    cursor_width: mapped.map_or(image.width, |value| value.2),
    cursor_height: mapped.map_or(image.height, |value| value.3),
    cursor_source_width: image.width,
    cursor_source_height: image.height,
    ..Default::default()
  })
}

pub(super) fn cursor_rgba(cursor: &CursorPreview) -> Result<CapturedImage, String> {
  decoded_rgba(&cursor.pixels)
}

pub(super) fn decoded_rgba(encoded: &[u8]) -> Result<CapturedImage, String> {
  let rgba = image::load_from_memory(encoded)
    .map_err(|error| error.to_string())?
    .into_rgba8();
  let (width, height) = rgba.dimensions();
  Ok(CapturedImage {
    height,
    rgba: rgba.into_raw(),
    width,
  })
}
