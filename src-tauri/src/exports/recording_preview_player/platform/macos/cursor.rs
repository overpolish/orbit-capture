// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Cursor;

use super::frame::CursorPreview;
use crate::exports::cursor_effects::{CursorCompositor, CursorEffectSettings, CursorOverlayCache};

pub(super) fn cursor_preview(
  cursor: Option<&CursorCompositor>,
  position_ms: u64,
  settings: CursorEffectSettings,
  output: (u32, u32),
  cache: &mut CursorOverlayCache,
) -> Result<Option<CursorPreview>, String> {
  let Some(cursor) = cursor.filter(|_| settings.bake) else {
    return Ok(None);
  };
  let layer_size = cursor.overlay_size(output.0 as usize, output.1 as usize, settings);
  let mut pixels = vec![0_u8; layer_size * layer_size * 4];
  let Some(position) = cursor.composite_overlay_bgra(
    &mut pixels,
    layer_size,
    (output.0 as usize, output.1 as usize),
    position_ms,
    settings,
    cache,
  ) else {
    return Ok(None);
  };
  for pixel in pixels.chunks_exact_mut(4) {
    pixel.swap(0, 2);
  }
  let image = image::RgbaImage::from_raw(layer_size as u32, layer_size as u32, pixels)
    .ok_or_else(|| "The cursor preview pixels are invalid".to_owned())?;
  let mut encoded = Cursor::new(Vec::new());
  image::DynamicImage::ImageRgba8(image)
    .write_to(&mut encoded, image::ImageFormat::Png)
    .map_err(|error| error.to_string())?;
  Ok(Some(CursorPreview {
    canvas_height: output.1,
    canvas_width: output.0,
    pixels: encoded.into_inner(),
    x: position.x,
    y: position.y,
  }))
}
