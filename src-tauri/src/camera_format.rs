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

/// How far down the wish list a cadence sits; anything unwished-for ranks last.
///
/// Preference order beats raw distance when picking one format per resolution:
/// under PAL lighting a camera that cannot reach 50 should drop to 25, not to
/// the nearer 30.
pub(crate) fn preference_rank(preferred: &[u32], fps: u32) -> usize {
  preferred
    .iter()
    .position(|candidate| *candidate == fps)
    .unwrap_or(preferred.len())
}

/// The rate the ranking treats as "the one asked for"; the rest of the list is
/// fallback, and a mode reached through it is already ordered by preference.
pub(crate) fn leading_fps(preferred: &[u32]) -> u32 {
  preferred.first().copied().unwrap_or(30)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn available_camera_formats(
  index: &CameraIndex,
  preferred_fps: &[u32],
) -> Result<Vec<CameraFormat>, String> {
  let fallback = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
  let mut camera = Camera::new(index.clone(), fallback).map_err(|error| error.to_string())?;
  let mut formats = camera
    .compatible_camera_formats()
    .map_err(|error| error.to_string())?;
  formats.retain(|format| RgbAFormat::FORMATS.contains(&format.format()));
  retain_preferred_per_resolution(&mut formats, preferred_fps);
  Ok(formats)
}

/// Keeps one format per native resolution — the earliest advertised preference,
/// or failing that the cadence closest to the leading one — largest first.
///
/// Duplicate pixel formats are not meaningful options to a person, and the
/// writer receives NV12 either way.
#[cfg(any(test, not(target_os = "macos")))]
fn retain_preferred_per_resolution(formats: &mut Vec<CameraFormat>, preferred: &[u32]) {
  let requested_fps = leading_fps(preferred);
  formats.sort_by_key(|format| {
    let resolution = format.resolution();
    (
      resolution.width(),
      resolution.height(),
      preference_rank(preferred, format.frame_rate()),
      format.frame_rate().abs_diff(requested_fps),
    )
  });
  formats.dedup_by(|left, right| left.resolution() == right.resolution());
  formats.sort_by_key(|format| {
    let resolution = format.resolution();
    std::cmp::Reverse(u64::from(resolution.width()) * u64::from(resolution.height()))
  });
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn resolve_exact_camera_format(
  index: &CameraIndex,
  width: u32,
  height: u32,
  fps: u32,
) -> Result<CameraFormat, String> {
  available_camera_formats(index, &[fps])?
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

  #[test]
  fn keeps_the_earliest_supported_preference_per_resolution() {
    let mut formats = vec![
      format(1920, 1080, 25),
      format(1920, 1080, 50),
      format(1280, 720, 30),
      format(1280, 720, 25),
    ];

    retain_preferred_per_resolution(&mut formats, &[50, 25]);

    assert_eq!(
      formats
        .iter()
        .map(|format| (format.resolution(), format.frame_rate()))
        .collect::<Vec<_>>(),
      vec![
        (Resolution::new(1920, 1080), 50),
        (Resolution::new(1280, 720), 25),
      ]
    );
  }

  #[test]
  fn takes_a_trailing_standard_rate_over_the_cadence_nearest_the_leading_one() {
    // A Media Foundation webcam lists 24 and 30 but no PAL rate; the wish list
    // ends in the standard rates so it lands on 30, not the nearer 24.
    let mut formats = vec![format(1920, 1080, 24), format(1920, 1080, 30)];

    retain_preferred_per_resolution(&mut formats, &[25, 30]);

    assert_eq!(formats.len(), 1);
    assert_eq!(formats[0].frame_rate(), 30);
  }

  #[test]
  fn falls_back_to_the_closest_cadence_when_no_preference_is_advertised() {
    let mut formats = vec![format(1920, 1080, 30), format(1920, 1080, 15)];

    retain_preferred_per_resolution(&mut formats, &[50, 25]);

    assert_eq!(formats.len(), 1);
    assert_eq!(formats[0].frame_rate(), 30);
  }
}
