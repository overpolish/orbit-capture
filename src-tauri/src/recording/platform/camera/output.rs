// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use cidre::{av, av::capture::VideoDataOutputSampleBufDelegate, objc};
use std::sync::atomic::AtomicBool;

use super::CameraSpec;
use crate::recording::platform::output::FrameClock;

/// Let exposure, focus, and white balance settle before time zero. A duration
/// is intentional: frame-count warmups become too short on a fast camera and
/// painfully long on a slow one.
const WARMUP_DURATION: Duration = Duration::from_millis(500);
const WARMUP_MIN_FRAMES: usize = 4;

pub(super) fn create(
  cancelled: Arc<AtomicBool>,
  commands: SyncSender<Command>,
  spec: &CameraSpec,
  started: mpsc::Sender<Result<(), String>>,
  stats: Arc<CaptureStats>,
) -> arc::R<CameraOutput> {
  CameraOutput::with(CameraOutputInner {
    cancelled,
    commands,
    expected_height: spec.height,
    expected_width: spec.width,
    first_frame_at: None,
    frame_count: 0,
    started: Some(started),
    stats,
  })
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
  pub(super) CameraOutput + av::capture::VideoDataOutputSampleBufDelegateImpl,
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
