// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

use super::{RecognizedCharacter, RecognizedLine, TextRect};

const OCR_PADDING: u32 = 32;

struct PaddedImage {
  height: u32,
  padding: u32,
  rgba: Vec<u8>,
  width: u32,
}

pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<Vec<RecognizedLine>, String> {
  let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| error.to_string())?;
  let max_dimension = OcrEngine::MaxImageDimension().map_err(|error| error.to_string())?;
  let image = pad_for_ocr(rgba, width, height, max_dimension)?;
  let writer = DataWriter::new().map_err(|error| error.to_string())?;
  writer
    .WriteBytes(&image.rgba)
    .map_err(|error| error.to_string())?;
  let buffer = writer.DetachBuffer().map_err(|error| error.to_string())?;
  let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
    &buffer,
    BitmapPixelFormat::Rgba8,
    image.width as i32,
    image.height as i32,
  )
  .map_err(|error| error.to_string())?;
  let result = engine
    .RecognizeAsync(&bitmap)
    .map_err(|error| error.to_string())?
    .join()
    .map_err(|error| error.to_string())?;
  let native_lines = result.Lines().map_err(|error| error.to_string())?;
  let mut lines = Vec::with_capacity(native_lines.Size().unwrap_or_default() as usize);
  for index in 0..native_lines.Size().map_err(|error| error.to_string())? {
    let line = native_lines
      .GetAt(index)
      .map_err(|error| error.to_string())?;
    let words = line.Words().map_err(|error| error.to_string())?;
    let mut left = f32::MAX;
    let mut top = f32::MAX;
    let mut right = 0.0_f32;
    let mut bottom = 0.0_f32;
    let mut characters = Vec::new();
    let mut text_offset = 0;
    for word_index in 0..words.Size().map_err(|error| error.to_string())? {
      let word = words.GetAt(word_index).map_err(|error| error.to_string())?;
      let bounds = word.BoundingRect().map_err(|error| error.to_string())?;
      let word_text = word.Text().map_err(|error| error.to_string())?.to_string();
      let word_left = (bounds.X - image.padding as f32).clamp(0.0, width as f32);
      let word_top = (bounds.Y - image.padding as f32).clamp(0.0, height as f32);
      let word_right = (bounds.X + bounds.Width - image.padding as f32).clamp(0.0, width as f32);
      let word_bottom = (bounds.Y + bounds.Height - image.padding as f32).clamp(0.0, height as f32);
      if word_right <= word_left || word_bottom <= word_top {
        continue;
      }
      left = left.min(word_left);
      top = top.min(word_top);
      right = right.max(word_right);
      bottom = bottom.max(word_bottom);
      let utf16_length = word_text.encode_utf16().count();
      for offset in 0..utf16_length {
        characters.push(RecognizedCharacter {
          bounds: TextRect {
            height: f64::from(word_bottom - word_top) / f64::from(height),
            width: f64::from(word_right - word_left) / utf16_length as f64 / f64::from(width),
            x: (f64::from(word_left)
              + f64::from(word_right - word_left) * offset as f64 / utf16_length as f64)
              / f64::from(width),
            y: f64::from(word_top) / f64::from(height),
          },
          end: text_offset + offset + 1,
          start: text_offset + offset,
        });
      }
      text_offset += utf16_length + 1;
    }
    if left == f32::MAX {
      continue;
    }
    lines.push(RecognizedLine {
      bounds: TextRect {
        height: f64::from(bottom - top) / f64::from(height),
        width: f64::from(right - left) / f64::from(width),
        x: f64::from(left) / f64::from(width),
        y: f64::from(top) / f64::from(height),
      },
      characters,
      confidence: 1.0,
      text: line.Text().map_err(|error| error.to_string())?.to_string(),
    });
  }
  Ok(lines)
}

fn pad_for_ocr(
  rgba: &[u8],
  width: u32,
  height: u32,
  max_dimension: u32,
) -> Result<PaddedImage, String> {
  if width == 0 || height == 0 {
    return Err("Text recognition image dimensions must be non-zero".to_string());
  }
  let source_length = width
    .checked_mul(height)
    .and_then(|pixels| pixels.checked_mul(4))
    .and_then(|length| usize::try_from(length).ok())
    .ok_or_else(|| "Text recognition image dimensions are too large".to_string())?;
  if rgba.len() != source_length {
    return Err("Text recognition image data has an unexpected size".to_string());
  }

  let horizontal_room = max_dimension.saturating_sub(width) / 2;
  let vertical_room = max_dimension.saturating_sub(height) / 2;
  let padding = OCR_PADDING.min(horizontal_room).min(vertical_room);
  let padded_width = width + padding * 2;
  let padded_height = height + padding * 2;
  let padded_length = padded_width
    .checked_mul(padded_height)
    .and_then(|pixels| pixels.checked_mul(4))
    .and_then(|length| usize::try_from(length).ok())
    .ok_or_else(|| "Padded text recognition image dimensions are too large".to_string())?;
  let background = border_background(rgba, width, height);
  let mut padded = vec![0; padded_length];
  for pixel in padded.chunks_exact_mut(4) {
    pixel.copy_from_slice(&background);
  }

  let source_stride = width as usize * 4;
  let padded_stride = padded_width as usize * 4;
  for row in 0..height as usize {
    let source_start = row * source_stride;
    let target_start = (row + padding as usize) * padded_stride + padding as usize * 4;
    padded[target_start..target_start + source_stride]
      .copy_from_slice(&rgba[source_start..source_start + source_stride]);
  }

  Ok(PaddedImage {
    height: padded_height,
    padding,
    rgba: padded,
    width: padded_width,
  })
}

fn border_background(rgba: &[u8], width: u32, height: u32) -> [u8; 4] {
  if width == 0 || height == 0 {
    return [255, 255, 255, 255];
  }

  let mut channels = [Vec::new(), Vec::new(), Vec::new()];
  let mut add_pixel = |x: u32, y: u32| {
    let index = (y as usize * width as usize + x as usize) * 4;
    for channel in 0..3 {
      channels[channel].push(rgba[index + channel]);
    }
  };
  for x in 0..width {
    add_pixel(x, 0);
    if height > 1 {
      add_pixel(x, height - 1);
    }
  }
  for y in 1..height.saturating_sub(1) {
    add_pixel(0, y);
    if width > 1 {
      add_pixel(width - 1, y);
    }
  }

  let mut background = [0, 0, 0, 255];
  for channel in 0..3 {
    channels[channel].sort_unstable();
    background[channel] = channels[channel][channels[channel].len() / 2];
  }
  background
}
