// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows camera capture into the native Media Foundation video writer.
//!
//! Media Foundation exposes ordinary webcams to Nokhwa as native YUYV/MJPEG
//! buffers. Decoding those device-owned bytes is the one unavoidable CPU
//! boundary; the resulting BGRA frame is uploaded directly to D3D11 and the
//! existing hardware H.264 writer owns everything downstream.

mod confidence;

use std::sync::{
  atomic::{AtomicBool, Ordering},
  mpsc, Arc, OnceLock,
};
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use nokhwa::{
  pixel_format::RgbAFormat,
  query,
  utils::{ApiBackend, RequestedFormat, RequestedFormatType},
  CallbackCamera,
};
use rayon::prelude::*;
use windows::Win32::Graphics::{
  Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
  },
  Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
};

use super::writer::{Command, Frame};
use crate::camera_format::resolve_exact_camera_format;
use crate::recording::{encoding::FailureReport, monitor::RecordingMonitor, CameraCaptureMode};
use crate::recording_inputs::camera_id;

const START_TIMEOUT: Duration = Duration::from_secs(8);
const WARMUP_DURATION: Duration = Duration::from_millis(500);
const WARMUP_MIN_FRAMES: u64 = 4;

pub(super) struct CameraSpec {
  device_id: String,
  index: nokhwa::utils::CameraIndex,
  pub(super) flipped: bool,
  pub(super) fps: u32,
  pub(super) height: u32,
  pal: bool,
  pub(super) width: u32,
}

impl CameraSpec {
  pub(super) fn resolve(mode: CameraCaptureMode) -> Result<Self, String> {
    let info = query(ApiBackend::Auto)
      .map_err(|error| error.to_string())?
      .into_iter()
      .find(|info| camera_id(info) == mode.device_id)
      .ok_or_else(|| "The selected camera is no longer available".to_owned())?;
    let width = mode.width & !1;
    let height = mode.height & !1;
    if width < 2 || height < 2 {
      return Err("The selected camera mode has no recordable area".to_owned());
    }
    Ok(Self {
      device_id: mode.device_id,
      index: info.index().clone(),
      flipped: mode.flipped,
      fps: mode.fps.max(1),
      height,
      pal: mode.pal,
      width,
    })
  }
}

pub(super) struct CameraStream {
  cancelled: Arc<AtomicBool>,
  confidence: Option<confidence::ConfidenceWorker>,
  worker: Option<std::thread::JoinHandle<()>>,
}

impl CameraStream {
  pub(super) fn stop(mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
    if let Some(confidence) = self.confidence.take() {
      confidence.stop();
    }
  }
}

impl Drop for CameraStream {
  fn drop(&mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
    if let Some(confidence) = self.confidence.take() {
      confidence.stop();
    }
  }
}

struct CallbackState {
  first_frame_at: Option<Instant>,
  frame_count: u64,
  warmup_announced: bool,
}

fn warmup_complete(frame_count: u64, elapsed: Duration) -> bool {
  frame_count >= WARMUP_MIN_FRAMES && elapsed >= WARMUP_DURATION
}

pub(super) fn start(
  spec: CameraSpec,
  device: ID3D11Device,
  commands: mpsc::SyncSender<Command>,
  timeline_origin: Arc<OnceLock<Instant>>,
  monitor: Arc<RecordingMonitor>,
  on_failure: FailureReport,
) -> Result<CameraStream, String> {
  let format = resolve_exact_camera_format(&spec.index, spec.width, spec.height, spec.fps)?;
  // Anti-flicker lives in the camera on Windows, not in the cadence; a camera
  // without the control (virtual cameras) still records, so this only reports.
  if let Err(error) =
    crate::camera_power_line::apply_power_line_frequency(&spec.device_id, spec.pal)
  {
    eprintln!("The camera's power line frequency was not set: {error}");
  }
  let confidence = confidence::ConfidenceWorker::spawn(Arc::clone(&monitor))?;
  let confidence_frames = confidence.sender();
  let cancelled = Arc::new(AtomicBool::new(false));
  let owner_cancelled = Arc::clone(&cancelled);
  let callback_cancelled = Arc::clone(&cancelled);
  let failure_reported = Arc::new(AtomicBool::new(false));
  let callback_failure_reported = Arc::clone(&failure_reported);
  let (started_tx, started) = mpsc::channel();
  let worker = std::thread::Builder::new()
    .name("screenwide-camera-capture-windows".to_owned())
    .spawn(move || {
      let requested = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::Exact(format));
      let capture_started = Instant::now();
      let mut state = CallbackState {
        first_frame_at: None,
        frame_count: 0,
        warmup_announced: false,
      };
      let mut camera = match CallbackCamera::new(spec.index, requested, move |frame| {
        if callback_cancelled.load(Ordering::Acquire) {
          return;
        }
        let wall = Instant::now();
        let (frame_wall, source_100ns) = camera_frame_clock(
          frame.capture_timestamp(),
          wall,
          SystemTime::now().duration_since(UNIX_EPOCH).ok(),
          capture_started,
        );
        let first_frame_at = *state.first_frame_at.get_or_insert(wall);
        state.frame_count = state.frame_count.saturating_add(1);
        if !warmup_complete(state.frame_count, wall.duration_since(first_frame_at)) {
          return;
        }
        if !state.warmup_announced {
          state.warmup_announced = true;
          // The camera's warm-up boundary is the shared zero for every track.
          // Screen and audio samples captured before it are discarded, keeping
          // the finished streams aligned without delaying backend startup.
          let _ = timeline_origin.set(frame_wall);
        }
        let resolution = frame.resolution();
        if (resolution.width(), resolution.height()) != (spec.width, spec.height) {
          report_once(
            &callback_failure_reported,
            &on_failure,
            format!(
              "The camera delivered {} x {} instead of the selected {} x {} format",
              resolution.width(),
              resolution.height(),
              spec.width,
              spec.height,
            ),
          );
          callback_cancelled.store(true, Ordering::Release);
          return;
        }
        let result = frame
          .decode_image::<RgbAFormat>()
          .map_err(|error| error.to_string())
          .map(|image| image.into_raw())
          .and_then(|rgba| {
            let rgba = Arc::new(rgba);
            if monitor.is_subscribed() {
              let _ = confidence_frames.try_send(confidence::CameraFrame {
                flipped: spec.flipped,
                height: spec.height,
                rgba: Arc::clone(&rgba),
                width: spec.width,
              });
            }
            let bgra = bgra_pixels(&rgba, spec.width, spec.height, spec.flipped);
            texture(&device, spec.width, spec.height, &bgra)
          });
        match result {
          Ok(texture) => {
            match commands.try_send(Command::Frame(Frame {
              source_100ns,
              texture,
              wall: frame_wall,
            })) {
              Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
              Err(mpsc::TrySendError::Disconnected(_)) => {
                callback_cancelled.store(true, Ordering::Release);
              }
            }
          }
          Err(error) => report_once(&callback_failure_reported, &on_failure, error),
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
      drop(camera);
    })
    .map_err(|error| error.to_string())?;

  match started.recv_timeout(START_TIMEOUT) {
    Ok(Ok(())) => Ok(CameraStream {
      cancelled,
      confidence: Some(confidence),
      worker: Some(worker),
    }),
    Ok(Err(error)) => {
      cancelled.store(true, Ordering::Release);
      let _ = worker.join();
      confidence.stop();
      Err(error)
    }
    Err(_) => {
      cancelled.store(true, Ordering::Release);
      let _ = worker.join();
      confidence.stop();
      Err("The camera did not start in time".to_owned())
    }
  }
}

/// Media Foundation supplies an absolute timestamp derived from the sample's
/// native presentation time. Map it back onto Rust's monotonic clock so audio
/// and video describe when capture occurred, not when webcam decoding ended.
/// Implausible/missing device clocks retain the arrival-time fallback.
fn camera_frame_clock(
  captured_epoch: Option<Duration>,
  arrived: Instant,
  arrival_epoch: Option<Duration>,
  stream_started: Instant,
) -> (Instant, i64) {
  const MAX_CAPTURE_LATENCY: Duration = Duration::from_secs(2);
  if let (Some(captured_epoch), Some(arrival_epoch)) = (captured_epoch, arrival_epoch) {
    if let Some(age) = arrival_epoch.checked_sub(captured_epoch) {
      if age <= MAX_CAPTURE_LATENCY {
        let captured = arrived.checked_sub(age).unwrap_or(arrived);
        let source_100ns = i64::try_from(captured_epoch.as_nanos() / 100).unwrap_or(i64::MAX);
        return (captured, source_100ns);
      }
    }
  }
  (
    arrived,
    i64::try_from(arrived.saturating_duration_since(stream_started).as_nanos() / 100)
      .unwrap_or(i64::MAX),
  )
}

fn report_once(reported: &AtomicBool, report: &FailureReport, message: String) {
  if !reported.swap(true, Ordering::AcqRel) {
    report(format!("Camera recording failed: {message}"));
  }
}

fn texture(
  device: &ID3D11Device,
  width: u32,
  height: u32,
  bgra: &[u8],
) -> Result<ID3D11Texture2D, String> {
  let expected = width as usize * height as usize * 4;
  if bgra.len() != expected {
    return Err("The camera produced an incomplete video frame".to_owned());
  }
  let description = D3D11_TEXTURE2D_DESC {
    Width: width,
    Height: height,
    MipLevels: 1,
    ArraySize: 1,
    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
    SampleDesc: DXGI_SAMPLE_DESC {
      Count: 1,
      Quality: 0,
    },
    Usage: D3D11_USAGE_DEFAULT,
    // Matches the screen-capture textures the sink writer already accepts. A
    // shader-resource-only texture is rejected by the Media Foundation video
    // pipeline with E_INVALIDARG when the sample reaches the encoder.
    BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
    ..Default::default()
  };
  let initial = D3D11_SUBRESOURCE_DATA {
    pSysMem: bgra.as_ptr().cast(),
    SysMemPitch: width * 4,
    ..Default::default()
  };
  let mut texture = None;
  unsafe { device.CreateTexture2D(&description, Some(&initial), Some(&mut texture)) }
    .map_err(|error| error.to_string())?;
  texture.ok_or_else(|| "D3D11 created no camera texture".to_owned())
}

fn bgra_pixels(rgba: &[u8], width: u32, height: u32, flipped: bool) -> Vec<u8> {
  let stride = width as usize * 4;
  let mut bgra = vec![0_u8; stride * height as usize];
  bgra
    .par_chunks_mut(stride)
    .enumerate()
    .for_each(|(row, output)| {
      let input = &rgba[row * stride..(row + 1) * stride];
      for x in 0..width as usize {
        let source_x = if flipped { width as usize - 1 - x } else { x };
        let source = &input[source_x * 4..source_x * 4 + 4];
        let target = &mut output[x * 4..x * 4 + 4];
        target.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
      }
    });
  bgra
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn camera_becomes_ready_only_after_frames_and_time_have_warmed_up() {
    assert!(!warmup_complete(WARMUP_MIN_FRAMES - 1, WARMUP_DURATION));
    assert!(!warmup_complete(
      WARMUP_MIN_FRAMES,
      WARMUP_DURATION - Duration::from_millis(1)
    ));
    assert!(warmup_complete(WARMUP_MIN_FRAMES, WARMUP_DURATION));
  }

  #[test]
  fn converts_rgba_to_native_bgra_and_mirrors_when_requested() {
    let rgba = [1, 2, 3, 4, 10, 20, 30, 40];
    assert_eq!(
      bgra_pixels(&rgba, 2, 1, false),
      [3, 2, 1, 4, 30, 20, 10, 40]
    );
    assert_eq!(bgra_pixels(&rgba, 2, 1, true), [30, 20, 10, 40, 3, 2, 1, 4]);
  }

  #[test]
  fn native_capture_time_removes_webcam_delivery_latency() {
    let arrived = Instant::now();
    let epoch = Duration::from_secs(10_000);
    let (captured, source) = camera_frame_clock(
      Some(epoch - Duration::from_millis(120)),
      arrived,
      Some(epoch),
      arrived - Duration::from_secs(1),
    );
    assert_eq!(arrived.duration_since(captured), Duration::from_millis(120));
    assert_eq!(source, 99_998_800_000);
  }

  #[test]
  fn implausible_camera_clock_uses_arrival_time() {
    let started = Instant::now();
    let arrived = started + Duration::from_millis(250);
    let (captured, source) = camera_frame_clock(
      Some(Duration::from_secs(1)),
      arrived,
      Some(Duration::from_secs(10)),
      started,
    );
    assert_eq!(captured, arrived);
    assert_eq!(source, 2_500_000);
  }
}
