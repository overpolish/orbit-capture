// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

use super::{RecognizedCharacter, RecognizedLine, TextRect};

pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<Vec<RecognizedLine>, String> {
  let writer = DataWriter::new().map_err(|error| error.to_string())?;
  writer.WriteBytes(rgba).map_err(|error| error.to_string())?;
  let buffer = writer.DetachBuffer().map_err(|error| error.to_string())?;
  let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
    &buffer,
    BitmapPixelFormat::Rgba8,
    width as i32,
    height as i32,
  )
  .map_err(|error| error.to_string())?;
  let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| error.to_string())?;
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
      left = left.min(bounds.X);
      top = top.min(bounds.Y);
      right = right.max(bounds.X + bounds.Width);
      bottom = bottom.max(bounds.Y + bounds.Height);
      let utf16_length = word_text.encode_utf16().count();
      for offset in 0..utf16_length {
        characters.push(RecognizedCharacter {
          bounds: TextRect {
            height: f64::from(bounds.Height) / f64::from(height),
            width: f64::from(bounds.Width) / utf16_length as f64 / f64::from(width),
            x: (f64::from(bounds.X)
              + f64::from(bounds.Width) * offset as f64 / utf16_length as f64)
              / f64::from(width),
            y: f64::from(bounds.Y) / f64::from(height),
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
