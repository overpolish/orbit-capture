// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cpal::{
  traits::{DeviceTrait, HostTrait},
  Device, SampleFormat, StreamConfig,
};
use nokhwa::{
  query,
  utils::{ApiBackend, CameraInfo},
};
use serde::Serialize;

#[cfg(target_os = "macos")]
use cidre::{av, ns};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceDetails {
  id: String,
  label: String,
  is_default: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraResolutionDetails {
  id: String,
  label: String,
  is_default: bool,
  width: u32,
  height: u32,
  fps: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDeviceDetails {
  id: String,
  label: String,
  is_default: bool,
  modes: Vec<CameraResolutionDetails>,
}

#[tauri::command]
pub async fn list_microphones() -> Result<Vec<InputDeviceDetails>, String> {
  tauri::async_runtime::spawn_blocking(enumerate_microphones)
    .await
    .map_err(|error| error.to_string())?
}

fn enumerate_microphones() -> Result<Vec<InputDeviceDetails>, String> {
  let host = cpal::default_host();
  let default_id = host
    .default_input_device()
    .and_then(|device| device.id().ok())
    .map(|id| id.to_string());
  let devices = host.input_devices().map_err(|error| error.to_string())?;
  let mut result = devices
    .filter_map(|device| {
      let id = device.id().ok()?.to_string();
      let label = device.description().ok()?.name().to_string();
      Some(InputDeviceDetails {
        is_default: default_id.as_deref() == Some(&id),
        id,
        label,
      })
    })
    .collect::<Vec<_>>();
  result.sort_by_cached_key(|device| (!device.is_default, device.label.to_lowercase()));
  result.dedup_by(|left, right| left.id == right.id);
  Ok(result)
}

pub(crate) fn resolve_microphone(
  device_id: Option<&str>,
) -> Result<(Device, StreamConfig, SampleFormat), String> {
  let host = cpal::default_host();
  let device = match device_id {
    Some(device_id) => host
      .input_devices()
      .map_err(|error| error.to_string())?
      .find(|device| {
        device
          .id()
          .is_ok_and(|candidate| candidate.to_string() == device_id)
      })
      .ok_or_else(|| "The selected microphone is no longer available".to_owned())?,
    None => host
      .default_input_device()
      .ok_or_else(|| "No default microphone is available".to_owned())?,
  };
  let config = device
    .default_input_config()
    .map_err(|error| error.to_string())?;
  let sample_format = config.sample_format();
  Ok((device, config.into(), sample_format))
}

#[tauri::command]
pub async fn list_cameras(fps: u32) -> Result<Vec<CameraDeviceDetails>, String> {
  tauri::async_runtime::spawn_blocking(move || enumerate_cameras(fps))
    .await
    .map_err(|error| error.to_string())?
}

fn enumerate_cameras(fps: u32) -> Result<Vec<CameraDeviceDetails>, String> {
  let cameras = query(ApiBackend::Auto).map_err(|error| error.to_string())?;
  let mut result = Vec::new();
  let mut has_default = false;
  for camera in cameras {
    let device_id = camera_id(&camera);
    let device_label = camera.human_name();
    let formats = camera_modes(&camera, fps);
    if formats.is_empty() {
      continue;
    }
    let preferred = preferred_mode(&formats, fps);
    let is_default = !has_default;
    has_default = true;
    let modes = formats
      .into_iter()
      .map(|(width, height, mode_fps)| CameraResolutionDetails {
        id: format!("{width}x{height}@{mode_fps}"),
        label: format!("{width} × {height}"),
        is_default: preferred == Some((width, height, mode_fps)),
        width,
        height,
        fps: mode_fps,
      })
      .collect();
    result.push(CameraDeviceDetails {
      id: device_id,
      label: device_label,
      is_default,
      modes,
    });
  }
  result.sort_by_cached_key(|camera| (!camera.is_default, camera.label.to_lowercase()));
  Ok(result)
}

fn preferred_mode(modes: &[(u32, u32, u32)], requested_fps: u32) -> Option<(u32, u32, u32)> {
  modes.iter().copied().min_by_key(|(width, height, fps)| {
    let aspect_error = u64::from(*width)
      .saturating_mul(9)
      .abs_diff(u64::from(*height).saturating_mul(16))
      .saturating_mul(1_000_000)
      / u64::from(*height).max(1);
    (
      fps.abs_diff(requested_fps),
      aspect_error,
      std::cmp::Reverse(u64::from(*width) * u64::from(*height)),
    )
  })
}

fn sort_camera_modes(modes: &mut [(u32, u32, u32)], requested_fps: u32) {
  modes.sort_by_key(|(width, height, fps)| {
    let orientation = match width.cmp(height) {
      std::cmp::Ordering::Greater => 0,
      std::cmp::Ordering::Equal => 1,
      std::cmp::Ordering::Less => 2,
    };
    (
      std::cmp::Reverse((*width).max(*height)),
      orientation,
      std::cmp::Reverse(u64::from(*width) * u64::from(*height)),
      fps.abs_diff(requested_fps),
    )
  });
}

#[cfg(target_os = "macos")]
fn camera_modes(camera: &CameraInfo, requested_fps: u32) -> Vec<(u32, u32, u32)> {
  let backend_id = ns::String::with_str(&camera_id(camera));
  let device = av::CaptureDevice::with_unique_id(&backend_id).or_else(|| {
    av::CaptureDevice::devices()
      .iter()
      .find(|device| device.localized_name().to_string() == camera.human_name())
      .map(|device| device.retained())
  });
  let Some(device) = device else {
    return Vec::new();
  };

  let mut modes = device
    .formats()
    .iter()
    .filter_map(|format| {
      let dimensions = format.format_desc().dims();
      let width = u32::try_from(dimensions.width).ok()?;
      let height = u32::try_from(dimensions.height).ok()?;
      let fps = format
        .video_supported_frame_rate_ranges()
        .iter()
        .map(|range| {
          let requested = f64::from(requested_fps);
          if range.min_frame_rate() <= requested && range.max_frame_rate() >= requested {
            requested_fps
          } else if requested < range.min_frame_rate() {
            range.min_frame_rate().ceil().max(1.0) as u32
          } else {
            range.max_frame_rate().floor().max(1.0) as u32
          }
        })
        .min_by_key(|fps| fps.abs_diff(requested_fps))?;
      Some((width, height, fps))
    })
    .collect::<Vec<_>>();
  modes.sort_by_key(|(width, height, fps)| (*width, *height, fps.abs_diff(requested_fps)));
  modes.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
  sort_camera_modes(&mut modes, requested_fps);
  modes
}

#[cfg(not(target_os = "macos"))]
fn camera_modes(camera: &CameraInfo, requested_fps: u32) -> Vec<(u32, u32, u32)> {
  let mut modes = crate::camera_format::available_camera_formats(camera.index(), requested_fps)
    .map_or_else(
      |_| Vec::new(),
      |formats| {
        formats
          .into_iter()
          .map(|format| {
            let resolution = format.resolution();
            (resolution.width(), resolution.height(), format.frame_rate())
          })
          .collect()
      },
    );
  sort_camera_modes(&mut modes, requested_fps);
  modes
}

pub(crate) fn camera_id(camera: &CameraInfo) -> String {
  let backend_id = camera.misc();
  if backend_id.trim().is_empty() {
    camera.index().as_string()
  } else {
    backend_id
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sorts_camera_modes_by_size_then_orientation() {
    let mut modes = vec![
      (640, 480, 60),
      (1080, 1920, 60),
      (1552, 1552, 60),
      (1760, 1328, 60),
      (1328, 1760, 60),
      (1920, 1080, 60),
    ];

    sort_camera_modes(&mut modes, 60);

    assert_eq!(
      modes,
      vec![
        (1920, 1080, 60),
        (1080, 1920, 60),
        (1760, 1328, 60),
        (1328, 1760, 60),
        (1552, 1552, 60),
        (640, 480, 60),
      ]
    );
  }
}
