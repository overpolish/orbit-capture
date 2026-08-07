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
use rayon::{
  iter::{IndexedParallelIterator, ParallelIterator},
  slice::ParallelSliceMut,
};
use tauri::{
  ipc::{Channel, InvokeResponseBody},
  AppHandle, Manager,
};
use yuv::{YuvPackedImage, YuvRange, YuvStandardMatrix};

use crate::recording_inputs::camera_id;

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
}

impl CameraPreviewWorker {
  fn cancel(&self) {
    self.cancelled.store(true, Ordering::Release);
  }
}

#[derive(Default)]
pub struct CameraPreviewState(Mutex<CameraPreviewManager>);

fn yuyv_to_rgba(buffer: &[u8], width: u32, height: u32) -> Vec<u8> {
  let yuyv_stride = width * 2;
  let rgba_stride = width * 4;
  let mut rgba_buffer = vec![0_u8; (width * height * 4) as usize];

  rgba_buffer
    .par_chunks_mut(rgba_stride as usize)
    .enumerate()
    .for_each(|(row_index, row_rgba)| {
      let input_offset = row_index * yuyv_stride as usize;
      let input_slice = &buffer[input_offset..input_offset + yuyv_stride as usize];
      let packed_image = YuvPackedImage {
        yuy: input_slice,
        yuy_stride: yuyv_stride,
        width,
        height: 1,
      };
      let _ = yuv::yuyv422_to_rgba(
        &packed_image,
        row_rgba,
        rgba_stride,
        YuvRange::Full,
        YuvStandardMatrix::Bt601,
      );
    });

  rgba_buffer
}

fn frame_payload(frame: Buffer) -> Result<Vec<u8>, String> {
  let resolution = frame.resolution();
  let is_mjpeg = frame.source_frame_format() == FrameFormat::MJPEG;
  let frame_data = match frame.source_frame_format() {
    FrameFormat::MJPEG => frame.buffer().to_vec(),
    FrameFormat::YUYV => yuyv_to_rgba(frame.buffer(), resolution.width(), resolution.height()),
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

fn build_camera_preview(device_id: &str, channel: Channel) -> Result<CameraPreviewWorker, String> {
  let camera_info = query(ApiBackend::Auto)
    .map_err(|error| error.to_string())?
    .into_iter()
    .find(|camera| camera_id(camera) == device_id)
    .ok_or_else(|| "The selected camera is no longer available".to_owned())?;
  let camera_index = camera_info.index().clone();
  let cancelled = Arc::new(AtomicBool::new(false));
  let owner_cancelled = Arc::clone(&cancelled);
  let callback_cancelled = Arc::clone(&cancelled);
  let (started_tx, started) = mpsc::channel();
  std::thread::Builder::new()
    .name("camera-preview".to_owned())
    .spawn(move || {
      let requested =
        RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
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

  let worker = CameraPreviewWorker { cancelled };
  started
    .recv()
    .map_err(|_| "The camera preview worker stopped before starting".to_owned())??;
  Ok(worker)
}

#[tauri::command]
pub async fn start_camera_preview(
  state: tauri::State<'_, CameraPreviewState>,
  device_id: String,
  channel: Channel,
) -> Result<(), String> {
  let generation = state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .begin_start();
  let worker =
    tauri::async_runtime::spawn_blocking(move || build_camera_preview(&device_id, channel))
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
