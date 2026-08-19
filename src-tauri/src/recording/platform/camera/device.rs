// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use super::CameraSpec;
use cidre::{av, ns};

pub(super) fn resolve(spec: &CameraSpec) -> Result<arc::R<av::CaptureDevice>, String> {
  crate::camera_frame_rate::resolve_device(&spec.device_id, &spec.device_name)
}

pub(super) fn configure(device: &mut av::CaptureDevice, spec: &CameraSpec) -> Result<(), String> {
  crate::camera_frame_rate::pin_frame_rate(device, spec.width, spec.height, spec.fps)
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
