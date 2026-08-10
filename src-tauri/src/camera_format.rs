// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Camera-format discovery shared by the picker, preview, and recording.
//!
//! Nokhwa's automatic request hides materially different native modes behind
//! one camera name. Discovery keeps one mode per resolution at the cadence
//! closest to the requested 30/60 fps; the UI presents every one, and both
//! preview and recording resolve the exact choice. No frame is resized.

#[cfg(any(test, not(target_os = "macos")))]
use nokhwa::utils::CameraFormat;
#[cfg(not(target_os = "macos"))]
use nokhwa::{
  pixel_format::{FormatDecoder, RgbAFormat},
  utils::{CameraIndex, RequestedFormat, RequestedFormatType},
  Camera,
};

#[cfg(test)]
const ASPECT_WIDTH: u64 = 16;
#[cfg(test)]
const ASPECT_HEIGHT: u64 = 9;

#[cfg(test)]
fn aspect_error(format: &CameraFormat) -> u64 {
  let resolution = format.resolution();
  let width = u64::from(resolution.width());
  let height = u64::from(resolution.height());
  // Normalising by height makes errors comparable between resolutions. The
  // multiplier retains enough precision for ordinary camera dimensions.
  width
    .saturating_mul(ASPECT_HEIGHT)
    .abs_diff(height.saturating_mul(ASPECT_WIDTH))
    .saturating_mul(1_000_000)
    / height.max(1)
}

#[cfg(test)]
pub(crate) fn preferred_camera_format(
  formats: &[CameraFormat],
  requested_fps: u32,
) -> Option<CameraFormat> {
  formats.iter().copied().min_by_key(|format| {
    let resolution = format.resolution();
    let pixels = u64::from(resolution.width()) * u64::from(resolution.height());
    (
      format.frame_rate().abs_diff(requested_fps),
      aspect_error(format),
      std::cmp::Reverse(pixels),
    )
  })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn available_camera_formats(
  index: &CameraIndex,
  requested_fps: u32,
) -> Result<Vec<CameraFormat>, String> {
  let fallback = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
  let mut camera = Camera::new(index.clone(), fallback).map_err(|error| error.to_string())?;
  let mut formats = camera
    .compatible_camera_formats()
    .map_err(|error| error.to_string())?;
  formats.retain(|format| RgbAFormat::FORMATS.contains(&format.format()));
  formats.sort_by_key(|format| {
    let resolution = format.resolution();
    (
      resolution.width(),
      resolution.height(),
      format.frame_rate().abs_diff(requested_fps),
    )
  });
  // For each native resolution retain the format whose advertised cadence is
  // closest to the bar's choice. Duplicate pixel formats are not meaningful
  // options to a person and AVFoundation produces NV12 for the writer anyway.
  formats.dedup_by(|left, right| left.resolution() == right.resolution());
  formats.sort_by_key(|format| {
    let resolution = format.resolution();
    std::cmp::Reverse(u64::from(resolution.width()) * u64::from(resolution.height()))
  });
  Ok(formats)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn resolve_exact_camera_format(
  index: &CameraIndex,
  width: u32,
  height: u32,
  fps: u32,
) -> Result<CameraFormat, String> {
  available_camera_formats(index, fps)?
    .into_iter()
    .find(|format| {
      let resolution = format.resolution();
      resolution.width() == width && resolution.height() == height && format.frame_rate() == fps
    })
    .ok_or_else(|| "The selected camera mode is no longer available".to_owned())
}

#[cfg(test)]
mod tests {
  use super::*;
  use nokhwa::utils::{FrameFormat, Resolution};

  fn format(width: u32, height: u32, fps: u32) -> CameraFormat {
    CameraFormat::new(Resolution::new(width, height), FrameFormat::NV12, fps)
  }

  #[test]
  fn prefers_native_sixteen_by_nine_over_a_larger_four_by_three_mode() {
    let formats = [format(1920, 1440, 60), format(1920, 1080, 60)];

    let selected = preferred_camera_format(&formats, 60).unwrap();

    assert_eq!(selected.resolution(), Resolution::new(1920, 1080));
    assert_eq!(selected.frame_rate(), 60);
  }

  #[test]
  fn requested_cadence_wins_before_aspect_ratio() {
    let formats = [format(1920, 1080, 30), format(1280, 960, 60)];

    let selected = preferred_camera_format(&formats, 60).unwrap();

    assert_eq!(selected.resolution(), Resolution::new(1280, 960));
    assert_eq!(selected.frame_rate(), 60);
  }

  #[test]
  fn takes_the_largest_native_resolution_after_cadence_and_aspect() {
    let formats = [format(1280, 720, 30), format(1920, 1080, 30)];

    let selected = preferred_camera_format(&formats, 30).unwrap();

    assert_eq!(selected.resolution(), Resolution::new(1920, 1080));
  }
}
