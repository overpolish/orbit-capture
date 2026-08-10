// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use cidre::av;
use nokhwa::{query, utils::ApiBackend};
use std::sync::atomic::AtomicBool;

use crate::recording_inputs::camera_id;

mod device;
mod output;

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
  let mut device = device::resolve(&spec)?;

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
  let delegate = output::create(callback_cancelled, commands, &spec, started, stats);
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
  let configuration = device::configure(&mut device, &spec)
    .and_then(|()| device::configure_output(&mut output, &spec));
  session.commit_cfg();
  configuration?;
  device::configure_mirroring(&output, flipped);

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
