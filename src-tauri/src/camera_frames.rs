// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Camera-frame conversion for the live preview.
//!
//! Nokhwa exposes the camera's native bytes. MJPEG is deliberately left
//! compressed; YUYV needs this conversion before the browser can display it.

use rayon::{
  iter::{IndexedParallelIterator, ParallelIterator},
  slice::ParallelSliceMut,
};
use yuv::{YuvPackedImage, YuvRange, YuvStandardMatrix};

pub(crate) fn yuyv_to_rgba(buffer: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
  let packed_stride = width
    .checked_mul(2)
    .ok_or_else(|| "The camera preview row is too wide".to_owned())?;
  let height_usize = usize::try_from(height).map_err(|error| error.to_string())?;
  if height_usize == 0 || !buffer.len().is_multiple_of(height_usize) {
    return Err("The camera preview has an invalid row layout".to_owned());
  }
  // CoreVideo aligns rows for some native dimensions (1552 square is one
  // example). Treating that padding as the next row is what produced the
  // diagonal, repeated bands in the preview.
  let yuyv_stride = buffer.len() / height_usize;
  if yuyv_stride < packed_stride as usize {
    return Err("The camera preview row is incomplete".to_owned());
  }
  let rgba_stride = width * 4;
  let mut rgba = vec![0_u8; (width * height * 4) as usize];

  rgba
    .par_chunks_mut(rgba_stride as usize)
    .enumerate()
    .for_each(|(row_index, row_rgba)| {
      let input_offset = row_index * yuyv_stride;
      let input = &buffer[input_offset..input_offset + packed_stride as usize];
      let packed = YuvPackedImage {
        yuy: input,
        yuy_stride: packed_stride,
        width,
        height: 1,
      };
      let _ = yuv::yuyv422_to_rgba(
        &packed,
        row_rgba,
        rgba_stride,
        YuvRange::Full,
        YuvStandardMatrix::Bt601,
      );
    });

  Ok(rgba)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ignores_core_video_row_padding() {
    let packed = [128, 128, 128, 128];
    let padded = [128, 128, 128, 128, 0, 0, 0, 0];

    assert_eq!(
      yuyv_to_rgba(&padded, 2, 1).unwrap(),
      yuyv_to_rgba(&packed, 2, 1).unwrap()
    );
  }
}
