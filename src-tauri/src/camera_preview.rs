// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
  },
  time::Duration,
};

use nokhwa::{
  pixel_format::RgbAFormat,
  query,
  utils::{ApiBackend, FrameFormat, RequestedFormat, RequestedFormatType},
  Buffer, CallbackCamera,
};

#[cfg(target_os = "macos")]
use nokhwa::utils::{CameraFormat, Resolution};
use tauri::{
  ipc::{Channel, InvokeResponseBody},
  AppHandle, Manager,
};

use crate::recording_inputs::camera_id;

#[cfg(not(target_os = "macos"))]
use crate::camera_format::resolve_exact_camera_format;

#[derive(Default)]
struct CameraPreviewManager {
  worker: Option<CameraPreviewWorker>,
  generation: u64,
}

impl CameraPreviewManager {
  fn begin_start(&mut self) -> u64 {
    self.generation = self.generation.wrapping_add(1);
    if let Some(worker) = self.worker.take() {
      worker.cancel();
    }
    self.generation
  }

  fn finish_start(&mut self, generation: u64, worker: CameraPreviewWorker) {
    if self.generation == generation {
      self.worker = Some(worker);
    } else {
      worker.cancel();
    }
  }

  fn cancel(&mut self) {
    self.generation = self.generation.wrapping_add(1);
    if let Some(worker) = self.worker.take() {
      worker.cancel();
    }
  }
}

struct CameraPreviewWorker {
  cancelled: Arc<AtomicBool>,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl CameraPreviewWorker {
  fn cancel(mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

#[derive(Default)]
pub struct CameraPreviewState(Mutex<CameraPreviewManager>);

fn frame_payload(frame: Buffer) -> Result<Vec<u8>, String> {
  let resolution = frame.resolution();
  let is_mjpeg = frame.source_frame_format() == FrameFormat::MJPEG;
  let frame_data = match frame.source_frame_format() {
    FrameFormat::MJPEG => frame.buffer().to_vec(),
    FrameFormat::YUYV => {
      crate::camera_frames::yuyv_to_rgba(frame.buffer(), resolution.width(), resolution.height())?
    }
    _ => frame
      .decode_image::<RgbAFormat>()
      .map_err(|error| error.to_string())?
      .into_raw(),
  };
  let mut payload = Vec::with_capacity(9 + frame_data.len());
  payload.extend_from_slice(&resolution.width().to_le_bytes());
  payload.extend_from_slice(&resolution.height().to_le_bytes());
  payload.push(u8::from(!is_mjpeg));
  payload.extend(frame_data);
  Ok(payload)
}

fn build_camera_preview(
  device_id: &str,
  width: u32,
  height: u32,
  fps: u32,
  channel: Channel,
) -> Result<CameraPreviewWorker, String> {
  let camera_info = query(ApiBackend::Auto)
    .map_err(|error| error.to_string())?
    .into_iter()
    .find(|camera| camera_id(camera) == device_id)
    .ok_or_else(|| "The selected camera is no longer available".to_owned())?;
  let camera_index = camera_info.index().clone();
  // AVFoundation already supplied this exact native mode during passive
  // enumeration. Constructing a Nokhwa Camera here just to enumerate it again
  // opens the device twice in immediate succession and can leave Continuity
  // cameras busy before the preview worker starts.
  #[cfg(target_os = "macos")]
  let format = CameraFormat::new(Resolution::new(width, height), FrameFormat::YUYV, fps);
  #[cfg(not(target_os = "macos"))]
  let format = resolve_exact_camera_format(&camera_index, width, height, fps)?;
  let cancelled = Arc::new(AtomicBool::new(false));
  let owner_cancelled = Arc::clone(&cancelled);
  let callback_cancelled = Arc::clone(&cancelled);
  let (started_tx, started) = mpsc::channel();
  let thread = std::thread::Builder::new()
    .name("camera-preview".to_owned())
    .spawn(move || {
      let requested = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Exact(format));
      let mut camera = match CallbackCamera::new(camera_index, requested, move |frame| {
        if callback_cancelled.load(Ordering::Acquire) {
          return;
        }
        if let Ok(payload) = frame_payload(frame) {
          if channel.send(InvokeResponseBody::Raw(payload)).is_err() {
            callback_cancelled.store(true, Ordering::Release);
          }
        }
      }) {
        Ok(camera) => camera,
        Err(error) => {
          let _ = started_tx.send(Err(error.to_string()));
          return;
        }
      };
      if let Err(error) = camera.open_stream() {
        let _ = started_tx.send(Err(error.to_string()));
        return;
      }
      if started_tx.send(Ok(())).is_err() {
        return;
      }

      while !owner_cancelled.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(5));
      }
      // CallbackCamera is closed by the same worker that created and owns it.
      drop(camera);
    })
    .map_err(|error| error.to_string())?;

  let worker = CameraPreviewWorker {
    cancelled,
    thread: Some(thread),
  };
  started
    .recv()
    .map_err(|_| "The camera preview worker stopped before starting".to_owned())??;
  Ok(worker)
}

#[tauri::command]
pub async fn start_camera_preview(
  state: tauri::State<'_, CameraPreviewState>,
  device_id: String,
  width: u32,
  height: u32,
  fps: u32,
  channel: Channel,
) -> Result<(), String> {
  let generation = state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .begin_start();
  let worker = tauri::async_runtime::spawn_blocking(move || {
    build_camera_preview(&device_id, width, height, fps, channel)
  })
  .await
  .map_err(|error| error.to_string())??;
  state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .finish_start(generation, worker);
  Ok(())
}

#[tauri::command]
pub fn stop_camera_preview(state: tauri::State<'_, CameraPreviewState>) -> Result<(), String> {
  state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .cancel();
  Ok(())
}

pub fn stop_all(app: &AppHandle) {
  if let Some(state) = app.try_state::<CameraPreviewState>() {
    if let Ok(mut manager) = state.0.lock() {
      manager.cancel();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use nokhwa::utils::Resolution;

  #[test]
  fn preserves_mjpeg_frames_with_a_binary_header() {
    let frame = Buffer::new(
      Resolution::new(2, 1),
      &[0xff, 0xd8, 0xff, 0xd9],
      FrameFormat::MJPEG,
    );
    let payload = frame_payload(frame).unwrap();
    assert_eq!(&payload[..4], &2_u32.to_le_bytes());
    assert_eq!(&payload[4..8], &1_u32.to_le_bytes());
    assert_eq!(payload[8], 0);
    assert_eq!(&payload[9..], &[0xff, 0xd8, 0xff, 0xd9]);
  }
}
