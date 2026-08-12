// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::io::Cursor;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use quantette::deps::palette::{cast::from_component_slice, Srgb};
use quantette::{ImageRef, Pipeline, QuantizeMethod};

use super::CapturedImage;

/// The largest palette an 8-bit indexed PNG can carry.
const MAX_PALETTE: usize = 256;

/// Applies an antialiased alpha mask without changing the captured dimensions.
pub fn rounded_corners(image: &CapturedImage, radius_percent: f64) -> CapturedImage {
  let radius = f64::from(image.width.min(image.height)) * radius_percent.clamp(0.0, 50.0) / 100.0;
  let mut rgba = image.rgba.clone();
  if radius <= 0.0 {
    return CapturedImage {
      height: image.height,
      rgba,
      width: image.width,
    };
  }

  for y in 0..image.height {
    for x in 0..image.width {
      let pixel_x = f64::from(x) + 0.5;
      let pixel_y = f64::from(y) + 0.5;
      let center_x = if pixel_x < radius {
        Some(radius)
      } else if pixel_x > f64::from(image.width) - radius {
        Some(f64::from(image.width) - radius)
      } else {
        None
      };
      let center_y = if pixel_y < radius {
        Some(radius)
      } else if pixel_y > f64::from(image.height) - radius {
        Some(f64::from(image.height) - radius)
      } else {
        None
      };
      let (Some(center_x), Some(center_y)) = (center_x, center_y) else {
        continue;
      };
      let distance = (pixel_x - center_x).hypot(pixel_y - center_y);
      let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
      let alpha = &mut rgba[((y * image.width + x) * 4 + 3) as usize];
      *alpha = (f64::from(*alpha) * coverage).round() as u8;
    }
  }

  CapturedImage {
    height: image.height,
    rgba,
    width: image.width,
  }
}

fn is_opaque(rgba: &[u8]) -> bool {
  rgba.chunks_exact(4).all(|pixel| pixel[3] == u8::MAX)
}

fn rgb_from_rgba(rgba: &[u8]) -> Vec<u8> {
  let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
  for pixel in rgba.chunks_exact(4) {
    rgb.extend_from_slice(&pixel[..3]);
  }
  rgb
}

/// An exact palette, for a still that already fits inside 256 colours.
///
/// A terminal, a solid window or a flat UI is bit-exact this way, rather than
/// being handed to a quantizer that can only approximate it. Anything richer
/// bails on the 257th colour, which for a real screenshot happens within the
/// first handful of pixels, so this costs nothing in the common case.
fn exact_palette(rgb: &[u8]) -> Option<(Vec<[u8; 3]>, Vec<u8>)> {
  let mut palette: Vec<[u8; 3]> = Vec::new();
  let mut index_of: HashMap<[u8; 3], u8> = HashMap::new();
  let mut indices = Vec::with_capacity(rgb.len() / 3);

  for pixel in rgb.chunks_exact(3) {
    let colour = [pixel[0], pixel[1], pixel[2]];
    let index = match index_of.get(&colour) {
      Some(index) => *index,
      None => {
        if palette.len() >= MAX_PALETTE {
          return None;
        }
        let index = u8::try_from(palette.len()).ok()?;
        palette.push(colour);
        index_of.insert(colour, index);
        index
      }
    };
    indices.push(index);
  }

  Some((palette, indices))
}

/// Reduces the still to a 256-colour palette.
///
/// k-means with Floyd-Steinberg dithering: the dithering is what keeps
/// wallpapers and photos from banding, and k-means is both markedly more
/// faithful than Wu and, run in parallel, faster than encoding the image
/// losslessly would have been.
fn quantized_palette(rgb: &[u8], width: u32, height: u32) -> Option<(Vec<[u8; 3]>, Vec<u8>)> {
  let colours = from_component_slice::<Srgb<u8>>(rgb);
  let image = ImageRef::new(width, height, colours).ok()?;
  let (palette, indices) = Pipeline::new()
    .quantize_method(QuantizeMethod::kmeans())
    .parallel(true)
    .input_image(image)
    .output_srgb8_indexed_image()
    .into_parts();

  Some((
    palette
      .into_iter()
      .map(|colour| [colour.red, colour.green, colour.blue])
      .collect(),
    indices,
  ))
}

fn encode_indexed_png(
  palette: &[[u8; 3]],
  indices: &[u8],
  width: u32,
  height: u32,
) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  {
    let mut encoder = png::Encoder::new(Cursor::new(&mut png), width, height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder.set_palette(palette.iter().flatten().copied().collect::<Vec<u8>>());
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
      .write_image_data(indices)
      .map_err(|error| error.to_string())?;
  }

  Ok(png)
}

pub(crate) fn encode_truecolor_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  let mut png = Vec::new();
  PngEncoder::new_with_quality(
    Cursor::new(&mut png),
    CompressionType::Default,
    FilterType::Sub,
  )
  .write_image(
    &image.rgba,
    image.width,
    image.height,
    ExtendedColorType::Rgba8,
  )
  .map_err(|error| error.to_string())?;

  Ok(png)
}

/// Encodes a still as PNG bytes.
///
/// Deliberately one function, so the compression backend can be swapped
/// without capture or saving noticing. A screen capture is almost always fully
/// opaque, so the alpha channel is dropped and the image is written as an
/// 8-bit indexed PNG - that indexing is where the size comes from, not the
/// quantization on its own. A capture that is not opaque keeps every channel
/// and is written losslessly instead, because the palette path has no alpha.
///
/// Measured on a synthetic 3456x2234 desktop: 4.57 MB originally, 2.78 MB
/// losslessly, 0.57 MB here, in ~390ms.
pub fn encode_png(image: &CapturedImage) -> Result<Vec<u8>, String> {
  if !is_opaque(&image.rgba) {
    return encode_truecolor_png(image);
  }

  let rgb = rgb_from_rgba(&image.rgba);
  let (palette, indices) = exact_palette(&rgb)
    .or_else(|| quantized_palette(&rgb, image.width, image.height))
    .ok_or_else(|| "The capture could not be reduced to a palette".to_owned())?;

  encode_indexed_png(&palette, &indices, image.width, image.height)
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;

  /// A still with `colours` distinct colours, tiled over the whole image.
  fn palette_image(width: u32, height: u32, colours: u32, alpha: u8) -> CapturedImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for pixel in 0..width * height {
      let colour = pixel % colours;
      rgba.extend_from_slice(&[
        (colour % 256) as u8,
        (colour / 256 % 256) as u8,
        (colour / 65_536 % 256) as u8,
        alpha,
      ]);
    }
    CapturedImage {
      rgba,
      width,
      height,
    }
  }

  /// A smooth gradient, which is far past what a 256-colour palette holds.
  fn gradient(width: u32, height: u32, alpha: u8) -> CapturedImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
      for x in 0..width {
        rgba.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128, alpha]);
      }
    }
    CapturedImage {
      rgba,
      width,
      height,
    }
  }

  fn png_info(png: &[u8]) -> (png::ColorType, png::BitDepth, u32, u32, usize) {
    let reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
    let info = reader.info();
    let palette = info.palette.as_ref().map_or(0, |palette| palette.len() / 3);
    (
      info.color_type,
      info.bit_depth,
      info.width,
      info.height,
      palette,
    )
  }

  #[test]
  fn writes_an_opaque_still_as_an_eight_bit_indexed_png() {
    let image = gradient(320, 200, 255);
    let png = encode_png(&image).unwrap();
    let (color, depth, width, height, palette) = png_info(&png);
    assert_eq!(color, png::ColorType::Indexed);
    assert_eq!(depth, png::BitDepth::Eight);
    assert_eq!((width, height), (320, 200));
    assert!(palette > 0 && palette <= MAX_PALETTE);
    assert!(png.len() < image.rgba.len());
  }

  #[test]
  fn keeps_a_still_that_already_fits_a_palette_bit_exact() {
    let image = palette_image(64, 64, 200, 255);
    let png = encode_png(&image).unwrap();
    let (color, _, _, _, palette) = png_info(&png);
    assert_eq!(color, png::ColorType::Indexed);
    assert_eq!(palette, 200);
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.into_raw(), image.rgba);
  }

  #[test]
  fn stays_close_to_the_original_when_it_has_to_quantize() {
    let image = gradient(256, 256, 255);
    let png = encode_png(&image).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (256, 256));
    let error: u64 = decoded
      .as_raw()
      .iter()
      .zip(&image.rgba)
      .map(|(decoded, original)| u64::from(decoded.abs_diff(*original)))
      .sum();
    let mean = error as f64 / image.rgba.len() as f64;
    assert!(mean < 8.0, "mean channel error {mean} is too high");
  }

  #[test]
  fn keeps_every_channel_when_the_still_is_not_opaque() {
    let image = gradient(64, 64, 128);
    let png = encode_png(&image).unwrap();
    let (color, _, _, _, _) = png_info(&png);
    assert_eq!(color, png::ColorType::Rgba);
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!(decoded.into_raw(), image.rgba);
  }

  #[test]
  fn rounds_only_the_requested_corners_with_antialiasing() {
    let image = palette_image(40, 20, 1, 255);
    let rounded = rounded_corners(&image, 50.0);
    let alpha = |x: u32, y: u32| rounded.rgba[((y * rounded.width + x) * 4 + 3) as usize];
    assert_eq!(alpha(0, 0), 0);
    assert_eq!(alpha(20, 10), 255);
    assert!(alpha(2, 3) > 0 && alpha(2, 3) < 255);
    assert_eq!(rounded.width, image.width);
    assert_eq!(rounded.height, image.height);
  }
}
