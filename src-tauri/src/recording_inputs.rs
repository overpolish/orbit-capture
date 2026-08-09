use cpal::{
  traits::{DeviceTrait, HostTrait},
  Device, SampleFormat, StreamConfig,
};
use nokhwa::{
  query,
  utils::{ApiBackend, CameraInfo},
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceDetails {
  id: String,
  label: String,
  is_default: bool,
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
pub async fn list_cameras() -> Result<Vec<InputDeviceDetails>, String> {
  tauri::async_runtime::spawn_blocking(enumerate_cameras)
    .await
    .map_err(|error| error.to_string())?
}

fn enumerate_cameras() -> Result<Vec<InputDeviceDetails>, String> {
  let mut result = query(ApiBackend::Auto)
    .map_err(|error| error.to_string())?
    .into_iter()
    .enumerate()
    .map(|(index, camera)| InputDeviceDetails {
      id: camera_id(&camera),
      is_default: index == 0,
      label: camera.human_name(),
    })
    .collect::<Vec<_>>();
  result.sort_by_cached_key(|device| (!device.is_default, device.label.to_lowercase()));
  result.dedup_by(|left, right| left.id == right.id);
  Ok(result)
}

pub(crate) fn camera_id(camera: &CameraInfo) -> String {
  let backend_id = camera.misc();
  if backend_id.trim().is_empty() {
    camera.index().as_string()
  } else {
    backend_id
  }
}
