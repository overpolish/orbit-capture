// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
  },
  time::Duration,
  time::Instant,
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
  /// Claims the next generation and hands back the worker it supersedes. The
  /// caller tears that worker down off the state lock so a slow camera close
  /// never blocks the thread that is holding it.
  fn begin_start(&mut self) -> (u64, Option<CameraPreviewWorker>) {
    self.generation = self.generation.wrapping_add(1);
    (self.generation, self.worker.take())
  }

  /// Stores the worker when it still matches the current generation, otherwise
  /// returns it so the caller can cancel it away from the lock.
  fn finish_start(
    &mut self,
    generation: u64,
    worker: CameraPreviewWorker,
  ) -> Option<CameraPreviewWorker> {
    if self.generation == generation {
      self.worker = Some(worker);
      None
    } else {
      Some(worker)
    }
  }

  fn take_worker(&mut self) -> Option<CameraPreviewWorker> {
    self.generation = self.generation.wrapping_add(1);
    self.worker.take()
  }

  fn cancel(&mut self) {
    if let Some(worker) = self.take_worker() {
      worker.cancel();
    }
  }
}

struct CameraPreviewWorker {
  cancelled: Arc<AtomicBool>,
  delivery: Option<PreviewDelivery>,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl CameraPreviewWorker {
  fn cancel(mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
    if let Some(delivery) = self.delivery.take() {
      delivery.stop();
    }
  }
}

#[derive(Default)]
pub struct CameraPreviewState(Mutex<CameraPreviewManager>);

const PREVIEW_MAX_WIDTH: u32 = 384;
const PREVIEW_MAX_HEIGHT: u32 = 240;
const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);

fn preview_dimensions(width: u32, height: u32) -> (u32, u32) {
  let scale = (f64::from(PREVIEW_MAX_WIDTH) / f64::from(width.max(1)))
    .min(f64::from(PREVIEW_MAX_HEIGHT) / f64::from(height.max(1)))
    .min(1.0);
  (
    (f64::from(width) * scale).round().max(1.0) as u32,
    (f64::from(height) * scale).round().max(1.0) as u32,
  )
}

fn frame_payload(frame: Buffer) -> Result<Vec<u8>, String> {
  let resolution = frame.resolution();
  let source_size = (resolution.width(), resolution.height());
  let target_size = preview_dimensions(source_size.0, source_size.1);
  let preserve_mjpeg =
    frame.source_frame_format() == FrameFormat::MJPEG && source_size == target_size;
  let (width, height, frame_data, rgba) = if preserve_mjpeg {
    (source_size.0, source_size.1, frame.buffer().to_vec(), false)
  } else {
    let decoded = match frame.source_frame_format() {
      FrameFormat::YUYV => image::RgbaImage::from_raw(
        source_size.0,
        source_size.1,
        crate::camera_frames::yuyv_to_rgba(frame.buffer(), source_size.0, source_size.1)?,
      )
      .ok_or_else(|| "The camera preview produced an incomplete image".to_owned())?,
      _ => frame
        .decode_image::<RgbAFormat>()
        .map_err(|error| error.to_string())?,
    };
    let decoded = if source_size == target_size {
      decoded
    } else {
      image::imageops::resize(
        &decoded,
        target_size.0,
        target_size.1,
        image::imageops::FilterType::Triangle,
      )
    };
    (target_size.0, target_size.1, decoded.into_raw(), true)
  };
  let mut payload = Vec::with_capacity(9 + frame_data.len());
  payload.extend_from_slice(&width.to_le_bytes());
  payload.extend_from_slice(&height.to_le_bytes());
  payload.push(u8::from(rgba));
  payload.extend(frame_data);
  Ok(payload)
}

const DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(100);

struct PreviewDelivery {
  cancelled: Arc<AtomicBool>,
  sender: Option<mpsc::SyncSender<Buffer>>,
  thread: Option<std::thread::JoinHandle<()>>,
}

impl PreviewDelivery {
  fn spawn(channel: Channel) -> Result<Self, String> {
    Self::spawn_with_sink(move |body| channel.send(body))
  }

  /// The delivery thread never waits on channel disconnect alone: nokhwa can
  /// permanently leak the thread that owns the frame callback, and that
  /// callback holds a sender clone. Polling the cancel flag keeps teardown
  /// bounded to `DELIVERY_POLL_INTERVAL` no matter how many senders survive.
  fn spawn_with_sink<S, E>(mut sink: S) -> Result<Self, String>
  where
    S: FnMut(InvokeResponseBody) -> Result<(), E> + Send + 'static,
  {
    let (sender, receiver) = mpsc::sync_channel::<Buffer>(0);
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    let thread = std::thread::Builder::new()
      .name("camera-preview-delivery".to_owned())
      .spawn(move || {
        let mut last_sent = None;
        loop {
          let frame = match receiver.recv_timeout(DELIVERY_POLL_INTERVAL) {
            Ok(frame) => {
              if thread_cancelled.load(Ordering::Acquire) {
                break;
              }
              frame
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
              if thread_cancelled.load(Ordering::Acquire) {
                break;
              }
              continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
          };
          let now = Instant::now();
          if last_sent.is_some_and(|last| now.duration_since(last) < PREVIEW_INTERVAL) {
            continue;
          }
          let Ok(payload) = frame_payload(frame) else {
            continue;
          };
          if sink(InvokeResponseBody::Raw(payload)).is_err() {
            break;
          }
          last_sent = Some(now);
        }
      })
      .map_err(|error| error.to_string())?;
    Ok(Self {
      cancelled,
      sender: Some(sender),
      thread: Some(thread),
    })
  }

  fn sender(&self) -> mpsc::SyncSender<Buffer> {
    self.sender.as_ref().expect("delivery is active").clone()
  }

  fn shutdown(&mut self) {
    self.cancelled.store(true, Ordering::Release);
    self.sender.take();
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }

  fn stop(mut self) {
    self.shutdown();
  }
}

impl Drop for PreviewDelivery {
  fn drop(&mut self) {
    self.shutdown();
  }
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
  let delivery = PreviewDelivery::spawn(channel)?;
  let preview_frames = delivery.sender();
  let (started_tx, started) = mpsc::channel();
  let thread = std::thread::Builder::new()
    .name("camera-preview".to_owned())
    .spawn(move || {
      let requested = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Exact(format));
      let mut camera = match CallbackCamera::new(camera_index, requested, move |frame| {
        if callback_cancelled.load(Ordering::Acquire) {
          return;
        }
        let _ = preview_frames.try_send(frame);
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
    delivery: Some(delivery),
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
  let (generation, previous) = state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .begin_start();
  let worker = tauri::async_runtime::spawn_blocking(move || {
    // The previous worker has to release the device before the new camera is
    // opened, otherwise the same device can still be busy.
    if let Some(previous) = previous {
      previous.cancel();
    }
    build_camera_preview(&device_id, width, height, fps, channel)
  })
  .await
  .map_err(|error| error.to_string())??;
  let stale = state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .finish_start(generation, worker);
  if let Some(stale) = stale {
    cancel_off_thread(stale).await?;
  }
  Ok(())
}

/// Camera teardown joins worker threads that can block for a while, so it must
/// never run on the caller's thread: `stop_camera_preview` is invoked on the
/// macOS main thread and would freeze the UI.
async fn cancel_off_thread(worker: CameraPreviewWorker) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || worker.cancel())
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn stop_camera_preview(state: tauri::State<'_, CameraPreviewState>) -> Result<(), String> {
  let worker = state
    .0
    .lock()
    .map_err(|_| "Camera preview state is unavailable".to_owned())?
    .take_worker();
  if let Some(worker) = worker {
    cancel_off_thread(worker).await?;
  }
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

  #[test]
  fn bounds_large_previews_without_changing_their_aspect() {
    assert_eq!(preview_dimensions(3_840, 2_160), (384, 216));
    assert_eq!(preview_dimensions(1_080, 1_920), (135, 240));
    assert_eq!(preview_dimensions(320, 180), (320, 180));
  }

  #[test]
  fn stops_delivery_while_a_leaked_sender_is_still_alive() {
    let delivery =
      PreviewDelivery::spawn_with_sink(|_: InvokeResponseBody| Ok::<(), ()>(())).unwrap();
    // Stands in for the sender clone that a leaked nokhwa frame callback keeps
    // alive: the channel never disconnects, so only the cancel flag can end
    // the delivery thread.
    let leaked = delivery.sender();
    let (stopped_tx, stopped) = mpsc::channel();
    std::thread::spawn(move || {
      delivery.stop();
      let _ = stopped_tx.send(());
    });
    assert!(
      stopped.recv_timeout(Duration::from_secs(2)).is_ok(),
      "the delivery thread did not exit while a sender clone was alive"
    );
    drop(leaked);
  }
}
