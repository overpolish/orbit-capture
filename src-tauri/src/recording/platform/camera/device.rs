// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use super::CameraSpec;
use cidre::{av, ns};

pub(super) fn resolve(spec: &CameraSpec) -> Result<arc::R<av::CaptureDevice>, String> {
  let unique_id = ns::String::with_str(&spec.device_id);
  if let Some(device) = av::CaptureDevice::with_unique_id(&unique_id) {
    return Ok(device);
  }

  av::CaptureDevice::devices()
    .iter()
    .find(|device| device.localized_name().to_string() == spec.device_name)
    .map(|device| device.retained())
    .ok_or_else(|| "The selected camera is no longer available".to_owned())
}

pub(super) fn configure(device: &mut av::CaptureDevice, spec: &CameraSpec) -> Result<(), String> {
  let format = device
    .formats()
    .iter()
    .find(|format| {
      let dimensions = format.format_desc().dims();
      dimensions.width == spec.width as i32
        && dimensions.height == spec.height as i32
        && format
          .video_supported_frame_rate_ranges()
          .iter()
          .any(|range| {
            range.min_frame_rate() <= spec.fps as f64 && range.max_frame_rate() >= spec.fps as f64
          })
    })
    .map(|format| format.retained())
    .ok_or_else(|| "The selected camera format is no longer available".to_owned())?;
  let frame_duration = cm::Time::new(1, spec.fps as cm::TimeScale);
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

pub(super) fn configure_output(
  output: &mut av::capture::VideoDataOutput,
  spec: &CameraSpec,
) -> Result<(), String> {
  let pixel_format = ns::Number::with_u32(cv::PixelFormat::_420V.0);
  let width = ns::Number::with_u32(spec.width);
  let height = ns::Number::with_u32(spec.height);
  let mut settings = ns::DictionaryMut::<ns::String, ns::Id>::with_capacity(3);
  settings.insert(ns::str!(c"PixelFormatType"), &pixel_format);
  settings.insert(ns::str!(c"Width"), &width);
  settings.insert(ns::str!(c"Height"), &height);
  output
    .set_video_settings(Some(&settings))
    .map_err(|error| error.to_string())?;
  Ok(())
}

pub(super) fn configure_mirroring(output: &av::capture::VideoDataOutput, flipped: bool) {
  let connections = output.connections();
  let Some(connection) = connections.first() else {
    return;
  };
  let mut connection = connection.retained();
  if connection.is_video_mirroring_supported() {
    connection.set_automatically_adjusts_video_mirroring(false);
    connection.set_video_mirrored(flipped);
  }
}
