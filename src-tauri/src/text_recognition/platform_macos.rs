// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cidre::{cv, ns, vn};

use super::{RecognizedCharacter, RecognizedLine, TextRect};

fn text_rect(bounds: cidre::cg::Rect) -> TextRect {
  TextRect {
    height: bounds.size.height,
    width: bounds.size.width,
    x: bounds.origin.x,
    // Vision is normalized from the bottom-left; the UI is top-left.
    y: 1.0 - bounds.origin.y - bounds.size.height,
  }
}

fn clamp_rect(rect: TextRect, bounds: TextRect) -> Option<TextRect> {
  let left = rect.x.max(bounds.x);
  let top = rect.y.max(bounds.y);
  let right = (rect.x + rect.width).min(bounds.x + bounds.width);
  let bottom = (rect.y + rect.height).min(bounds.y + bounds.height);
  (right > left && bottom > top).then_some(TextRect {
    height: bottom - top,
    width: right - left,
    x: left,
    y: top,
  })
}

pub fn recognize(rgba: &[u8], width: u32, height: u32) -> Result<Vec<RecognizedLine>, String> {
  let mut pixel_buffer = cv::PixelBuf::new(
    width as usize,
    height as usize,
    cv::PixelFormat::_32_BGRA,
    None,
  )
  .map_err(|error| error.to_string())?;
  let flags = cv::pixel_buffer::LockFlags::DEFAULT;
  unsafe { pixel_buffer.lock_base_addr(flags) }
    .result()
    .map_err(|error| error.to_string())?;
  let stride = pixel_buffer.bytes_per_row();
  let base = unsafe { pixel_buffer.base_address_mut() } as *mut u8;
  for row in 0..height as usize {
    let target =
      unsafe { std::slice::from_raw_parts_mut(base.add(row * stride), width as usize * 4) };
    let source = &rgba[row * width as usize * 4..(row + 1) * width as usize * 4];
    for (source, target) in source.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
      target.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }
  }
  unsafe { pixel_buffer.unlock_lock_base_addr(flags) };

  let mut request = vn::RecognizeTextRequest::new();
  request.set_revision(vn::RecognizeTextRequest::REVISION_3);
  request.set_recognition_level(vn::RequestTextRecognitionLevel::Accurate);
  request.set_automatically_detects_lang(true);
  // Correction improves prose, but can silently rewrite punctuation and code.
  request.set_uses_lang_correction(false);
  let handler = vn::ImageRequestHandler::with_cv_pixel_buf(&pixel_buffer, None)
    .ok_or_else(|| "Vision could not read the selected image".to_owned())?;
  let requests = ns::Array::<vn::Request>::from_slice(&[&request]);
  handler
    .perform(&requests)
    .map_err(|error| error.map_or_else(|| "Vision failed".to_owned(), ToString::to_string))?;

  let mut lines = Vec::new();
  if let Some(observations) = request.results() {
    for observation in observations.iter() {
      let candidates = observation.top_candidates(1);
      let Some(candidate) = candidates.first() else {
        continue;
      };
      let text = candidate.string().to_string();
      let line_bounds = text_rect(observation.bounding_box());
      let mut utf16_offset = 0;
      let characters = text
        .chars()
        .filter_map(|character| {
          let length = character.len_utf16();
          let start = utf16_offset;
          utf16_offset += length;
          if character.is_whitespace() {
            return None;
          }
          candidate
            .bounding_box_for_range(ns::Range::new(start, length))
            .ok()
            .and_then(|observation| clamp_rect(text_rect(observation.bounding_box()), line_bounds))
            .map(|bounds| RecognizedCharacter {
              bounds,
              end: utf16_offset,
              start,
            })
        })
        .collect();
      lines.push(RecognizedLine {
        bounds: line_bounds,
        characters,
        confidence: candidate.confidence(),
        text,
      });
    }
  }
  lines.sort_by(|a, b| {
    a.bounds
      .y
      .total_cmp(&b.bounds.y)
      .then_with(|| a.bounds.x.total_cmp(&b.bounds.x))
  });
  Ok(lines)
}
