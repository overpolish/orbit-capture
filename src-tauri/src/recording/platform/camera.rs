// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use cidre::{av, av::capture::VideoDataOutputSampleBufDelegate, ns};
use nokhwa::{query, utils::ApiBackend};
use std::sync::atomic::AtomicBool;

use super::output::FrameClock;
use crate::recording_inputs::camera_id;

/// Let exposure, focus, and white balance settle before time zero. A duration
/// is intentional: frame-count warmups become too short on a fast camera and
/// painfully long on a slow one.
const WARMUP_DURATION: Duration = Duration::from_millis(500);
const WARMUP_MIN_FRAMES: usize = 4;
const START_TIMEOUT: Duration = Duration::from_secs(8);

pub(super) struct CameraSpec {
  device_id: String,
  device_name: String,
  pub(super) fps: u32,
  pub(super) height: u32,
  pub(super) width: u32,
}

impl CameraSpec {
  pub(super) fn resolve(mode: CameraCaptureMode) -> Result<Self, String> {
    let info = query(ApiBackend::Auto)
      .map_err(|error| error.to_string())?
      .into_iter()
      .find(|camera| camera_id(camera) == mode.device_id)
      .ok_or_else(|| "The selected camera is no longer available".to_owned())?;
    Ok(Self {
      device_id: camera_id(&info),
      device_name: info.human_name(),
      fps: mode.fps.max(1),
      height: even(mode.height),
      width: even(mode.width),
    })
  }
}

pub(super) struct CameraStream {
  cancelled: Arc<AtomicBool>,
  worker: Option<JoinHandle<()>>,
}

impl CameraStream {
  pub(super) fn stop(mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }
}

impl Drop for CameraStream {
  fn drop(&mut self) {
    self.cancelled.store(true, Ordering::Release);
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }
}

pub(super) fn start(
  spec: CameraSpec,
  flipped: bool,
  commands: SyncSender<Command>,
  stats: Arc<CaptureStats>,
) -> Result<CameraStream, String> {
  let cancelled = Arc::new(AtomicBool::new(false));
  let owner_cancelled = Arc::clone(&cancelled);
  let callback_cancelled = Arc::clone(&cancelled);
  let (started_tx, started) = mpsc::channel();
  let worker = std::thread::Builder::new()
    .name("orbit-camera-capture".to_owned())
    .spawn(move || {
      let result = run_capture(
        spec,
        flipped,
        commands,
        stats,
        callback_cancelled,
        started_tx.clone(),
        owner_cancelled,
      );
      if let Err(error) = result {
        let _ = started_tx.send(Err(error));
      }
    })
    .map_err(|error| error.to_string())?;

  match started.recv_timeout(START_TIMEOUT) {
    Ok(Ok(())) => Ok(CameraStream {
      cancelled,
      worker: Some(worker),
    }),
    Ok(Err(error)) => {
      cancelled.store(true, Ordering::Release);
      let _ = worker.join();
      Err(error)
    }
    Err(_) => {
      cancelled.store(true, Ordering::Release);
      let _ = worker.join();
      Err("The camera did not produce a stable frame in time".to_owned())
    }
  }
}

fn run_capture(
  spec: CameraSpec,
  flipped: bool,
  commands: SyncSender<Command>,
  stats: Arc<CaptureStats>,
  callback_cancelled: Arc<AtomicBool>,
  started: mpsc::Sender<Result<(), String>>,
  owner_cancelled: Arc<AtomicBool>,
) -> Result<(), String> {
  let mut device = resolve_device(&spec)?;

  let input = av::CaptureDeviceInput::with_device(&device).map_err(|error| error.to_string())?;
  let mut output = av::capture::VideoDataOutput::new();
  output.set_always_discard_late_video_frames(true);
  // The default lets AVFoundation substitute screen-sized preview buffers.
  // Those can have a different aspect ratio from the selected active format,
  // and AVAssetWriter then stretches them into the requested output size.
  output.set_automatically_configures_output_buf_dims(false);
  output
    .set_delivers_preview_sized_output_bufs(false)
    .map_err(|error| error.to_string())?;

  let queue = dispatch::Queue::serial_with_ar_pool();
  let delegate = CameraOutput::with(CameraOutputInner {
    cancelled: callback_cancelled,
    commands,
    expected_height: spec.height,
    expected_width: spec.width,
    first_frame_at: None,
    frame_count: 0,
    started: Some(started),
    stats,
  });
  output.set_sample_buf_delegate(Some(delegate.as_ref()), Some(&queue));

  let mut session = av::capture::Session::new();
  if !session.can_add_input(&input) {
    return Err("The selected camera cannot be added to a capture session".to_owned());
  }
  if !session.can_add_output(&output) {
    return Err("The camera cannot provide video frames".to_owned());
  }
  // Apple requires the device format and its frame durations to change inside
  // the same begin/commit transaction as the session inputs and outputs. Done
  // afterwards, the device remembers the choice for the *next* session while
  // this one can continue delivering its previous/default dimensions.
  session.begin_cfg();
  session.add_input(&input);
  session.add_output(&output);
  let configuration =
    configure_device(&mut device, &spec).and_then(|()| configure_output(&mut output, &spec));
  session.commit_cfg();
  configuration?;
  configure_mirroring(&output, flipped);

  session.start_running();
  while !owner_cancelled.load(Ordering::Acquire) {
    std::thread::sleep(Duration::from_millis(5));
  }
  session.stop_running();

  // Keep every Objective-C object alive until the session has fully stopped.
  drop(delegate);
  drop(output);
  drop(input);
  drop(queue);
  Ok(())
}

fn resolve_device(spec: &CameraSpec) -> Result<arc::R<av::CaptureDevice>, String> {
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

fn configure_device(device: &mut av::CaptureDevice, spec: &CameraSpec) -> Result<(), String> {
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

fn configure_output(
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

fn configure_mirroring(output: &av::capture::VideoDataOutput, flipped: bool) {
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

#[repr(C)]
struct CameraOutputInner {
  cancelled: Arc<AtomicBool>,
  commands: SyncSender<Command>,
  expected_height: u32,
  expected_width: u32,
  first_frame_at: Option<Instant>,
  frame_count: usize,
  started: Option<mpsc::Sender<Result<(), String>>>,
  stats: Arc<CaptureStats>,
}

impl CameraOutputInner {
  fn handle_frame(&mut self, sample: &cm::SampleBuf) {
    if self.cancelled.load(Ordering::Acquire) || !sample.data_is_ready() {
      return;
    }
    let Some(image) = sample.image_buf() else {
      return;
    };
    let actual_width = u32::try_from(image.width()).unwrap_or(u32::MAX);
    let actual_height = u32::try_from(image.height()).unwrap_or(u32::MAX);
    if (actual_width, actual_height) != (self.expected_width, self.expected_height) {
      if let Some(started) = self.started.take() {
        let _ = started.send(Err(format!(
          "The camera delivered {actual_width} × {actual_height} instead of the selected {} × {} format",
          self.expected_width, self.expected_height
        )));
      }
      self.cancelled.store(true, Ordering::Release);
      return;
    }
    let now = Instant::now();
    let first_frame_at = *self.first_frame_at.get_or_insert(now);
    self.frame_count += 1;
    if self.frame_count < WARMUP_MIN_FRAMES || now.duration_since(first_frame_at) < WARMUP_DURATION
    {
      return;
    }

    if let Some(started) = self.started.take() {
      let _ = started.send(Ok(()));
    }
    let clock = time_to_ns(sample.pts()).map_or(FrameClock::Wall, FrameClock::Source);
    let frame = Frame {
      buf: image.retained(),
      clock,
      wall: now,
    };
    if let Err(TrySendError::Full(_)) = self.commands.try_send(Command::Frame(frame)) {
      self.stats.dropped.fetch_add(1, Ordering::Relaxed);
    }
  }
}

define_obj_type!(
  CameraOutput + av::capture::VideoDataOutputSampleBufDelegateImpl,
  CameraOutputInner,
  CAMERA_OUTPUT_CLS
);

impl av::capture::VideoDataOutputSampleBufDelegate for CameraOutput {}

#[objc::add_methods]
impl av::capture::VideoDataOutputSampleBufDelegateImpl for CameraOutput {
  extern "C" fn impl_capture_output_did_output_sample_buf_from_connection(
    &mut self,
    _command: Option<&objc::Sel>,
    _output: &av::CaptureOutput,
    sample: &cm::SampleBuf,
    _connection: &av::CaptureConnection,
  ) {
    self.inner_mut().handle_frame(sample);
  }

  extern "C" fn impl_capture_output_did_drop_sample_buf_from_connection(
    &mut self,
    _command: Option<&objc::Sel>,
    _output: &av::CaptureOutput,
    _sample: &cm::SampleBuf,
    _connection: &av::CaptureConnection,
  ) {
    self
      .inner()
      .stats
      .capture_dropped
      .fetch_add(1, Ordering::Relaxed);
  }
}
