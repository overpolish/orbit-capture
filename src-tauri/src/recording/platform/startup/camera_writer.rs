// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use super::writer_thread::{spawn_writer, WriterThread};
use super::{camera::CameraSpec, session::CameraObjects, writer::VideoSource};
use crate::recording::encoding::FailureReport;
use crate::recording::monitor::RecordingMonitor;

pub(super) struct CameraWriterSetup {
  pub first_frame: Option<Receiver<Result<(), String>>>,
  pub primary_spec: Option<CameraSpec>,
  pub secondary: Option<CameraObjects>,
}

pub(super) fn prepare(
  spec: Option<CameraSpec>,
  camera_primary: bool,
  camera_flipped: bool,
  camera_path: Option<PathBuf>,
  timeline_origin: &Arc<OnceLock<Instant>>,
  monitor: &Arc<RecordingMonitor>,
  on_failure: &FailureReport,
) -> Result<CameraWriterSetup, String> {
  let Some(spec) = spec else {
    return Ok(CameraWriterSetup {
      first_frame: None,
      primary_spec: None,
      secondary: None,
    });
  };
  if camera_primary {
    return Ok(CameraWriterSetup {
      first_frame: None,
      primary_spec: Some(spec),
      secondary: None,
    });
  }

  let path = camera_path.ok_or_else(|| "The camera has nowhere to record".to_owned())?;
  let stats = Arc::new(CaptureStats::default());
  let WriterThread {
    commands,
    first_frame,
    worker,
  } = spawn_writer(
    WriterConfig {
      path: path.clone(),
      width: spec.width,
      height: spec.height,
      fps: spec.fps,
      // Both concurrent video writers use HEVC so VideoToolbox can keep
      // independent hardware-backed sessions for multi-video capture on macOS.
      encoder: VideoEncoder::Hevc,
      system_audio: false,
      microphone_format: None,
      stats: Arc::clone(&stats),
      on_failure: Arc::clone(on_failure),
      container: Container::quicktime_fragmented(),
      primary_video: false,
      source: VideoSource::Camera,
      timeline_origin: Arc::clone(timeline_origin),
    },
    "screenwide-camera-writer",
  )?;
  let stream = camera::start(
    spec,
    camera_flipped,
    commands.clone(),
    Arc::clone(monitor),
    stats,
  )?;

  Ok(CameraWriterSetup {
    first_frame: Some(first_frame),
    primary_spec: None,
    secondary: Some(CameraObjects {
      commands,
      path,
      stream: Some(stream),
      worker: Some(worker),
    }),
  })
}
