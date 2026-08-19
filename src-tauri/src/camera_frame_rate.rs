// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared AVFoundation frame rate helpers for the recording and preview camera
//! paths. Both need to look up the same `av::CaptureDevice` and pin it to the
//! frame duration the app asked for, and the preview additionally has to know
//! which frame rate nokhwa will accept when it opens the device.

use cidre::{arc, av, cm, ns};

/// Nokhwa's AVFoundation backend compares the requested fps against a range's
/// `maxFrameRate` with this tolerance, so anything within it is accepted as-is.
const NOKHWA_FPS_TOLERANCE: f64 = 0.999;

pub(crate) fn resolve_device(
  device_id: &str,
  device_name: &str,
) -> Result<arc::R<av::CaptureDevice>, String> {
  let unique_id = ns::String::with_str(device_id);
  if let Some(device) = av::CaptureDevice::with_unique_id(&unique_id) {
    return Ok(device);
  }

  av::CaptureDevice::devices()
    .iter()
    .find(|device| device.localized_name().to_string() == device_name)
    .map(|device| device.retained())
    .ok_or_else(|| "The selected camera is no longer available".to_owned())
}

/// Selects the format that matches `width`/`height` and brackets `fps`, then
/// pins the active min and max frame duration to `1/fps`.
pub(crate) fn pin_frame_rate(
  device: &mut av::CaptureDevice,
  width: u32,
  height: u32,
  fps: u32,
) -> Result<(), String> {
  let format = device
    .formats()
    .iter()
    .find(|format| {
      let dimensions = format.format_desc().dims();
      dimensions.width == width as i32
        && dimensions.height == height as i32
        && format
          .video_supported_frame_rate_ranges()
          .iter()
          .any(|range| range.min_frame_rate() <= fps as f64 && range.max_frame_rate() >= fps as f64)
    })
    .map(|format| format.retained())
    .ok_or_else(|| "The selected camera format is no longer available".to_owned())?;
  let frame_duration = cm::Time::new(1, fps as cm::TimeScale);
  let mut lock = device.config_lock().map_err(|error| error.to_string())?;
  lock.set_active_format(&format);
  lock
    .set_active_video_min_frame_duration(frame_duration)
    .map_err(|error| error.to_string())?;
  lock
    .set_active_video_max_frame_duration(frame_duration)
    .map_err(|error| error.to_string())?;
  Ok(())
}

/// The frame rate nokhwa will accept when opening this mode.
///
/// Nokhwa only takes an fps that equals some range's `maxFrameRate`, so a
/// request for 25 or 50 fps against a 1–30 range (Continuity Camera) is
/// rejected outright. Opening at the range maximum keeps the preview alive; the
/// caller then pins the real frame duration afterwards. When nothing matches,
/// the requested fps is returned unchanged so nokhwa fails exactly as before.
pub(crate) fn nokhwa_frame_rate(
  device: &av::CaptureDevice,
  width: u32,
  height: u32,
  fps: u32,
) -> u32 {
  let target = fps as f64;
  let mut bracketing = None;
  for format in device.formats().iter() {
    let dimensions = format.format_desc().dims();
    if dimensions.width != width as i32 || dimensions.height != height as i32 {
      continue;
    }
    for range in format.video_supported_frame_rate_ranges().iter() {
      let max = range.max_frame_rate();
      if (max - target).abs() <= NOKHWA_FPS_TOLERANCE {
        return fps;
      }
      if bracketing.is_none() && range.min_frame_rate() <= target && max >= target {
        bracketing = Some(max.round() as u32);
      }
    }
  }
  bracketing.unwrap_or(fps)
}
