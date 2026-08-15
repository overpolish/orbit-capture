// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows screen recording: Windows Graphics Capture into Media Foundation.
//!
//! WGC hands D3D11 textures to a bounded channel. The capture callback never
//! waits for the encoder, and Media Foundation consumes those textures through
//! its DXGI device manager without a GPU-to-CPU copy or an FFmpeg subprocess.

mod audio;
mod camera;
mod capture;
mod writer;

use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use capture::{CaptureObjects, CaptureTarget};
use writer::{Command, WriterConfig};

use crate::capture_geometry::{physical_capture_rect, video_capture_rect, CaptureRect};

use super::encoding::FinalizeInfo;
use super::{
  cursor::{CursorSource, CursorSourceKind},
  CaptureStartupConfig, PrimaryCaptureSource,
};

const FINALIZE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct CaptureStart {
  pub cursor_source: Option<super::cursor::CursorSource>,
  pub first_frame: mpsc::Receiver<Result<(), String>>,
  pub session: CaptureSession,
  pub source_scale_factor: f32,
  pub timeline_origin: Arc<OnceLock<Instant>>,
}

pub struct CaptureSession {
  audio: Option<audio::AudioCaptures>,
  audio_only_clock: Option<AudioOnlyClock>,
  audio_only_path: Option<std::path::PathBuf>,
  camera: Option<CameraRecording>,
  capture: Option<CaptureObjects>,
  commands: Option<mpsc::SyncSender<Command>>,
  primary_camera: Option<camera::CameraStream>,
  stopped_at: Arc<OnceLock<Instant>>,
  worker: Option<JoinHandle<()>>,
}

struct CameraRecording {
  commands: mpsc::SyncSender<Command>,
  path: std::path::PathBuf,
  stream: Option<camera::CameraStream>,
  worker: Option<JoinHandle<()>>,
}

struct AudioOnlyClock {
  paused: Mutex<(Option<Instant>, Duration)>,
  started: Instant,
}

impl AudioOnlyClock {
  fn new(started: Instant) -> Self {
    Self {
      paused: Mutex::new((None, Duration::ZERO)),
      started,
    }
  }

  fn pause(&self, at: Instant) {
    if let Ok(mut state) = self.paused.lock() {
      state.0.get_or_insert(at);
    }
  }

  fn resume(&self, at: Instant) {
    if let Ok(mut state) = self.paused.lock() {
      if let Some(started) = state.0.take() {
        state.1 = state
          .1
          .saturating_add(at.saturating_duration_since(started));
      }
    }
  }

  fn duration_ms(&self, at: Instant) -> u64 {
    let elapsed = at.saturating_duration_since(self.started);
    let paused = self
      .paused
      .lock()
      .map(|state| {
        state.1.saturating_add(
          state
            .0
            .map_or(Duration::ZERO, |pause| at.saturating_duration_since(pause)),
        )
      })
      .unwrap_or(Duration::ZERO);
    u64::try_from(elapsed.saturating_sub(paused).as_millis()).unwrap_or(u64::MAX)
  }
}

impl CaptureSession {
  pub fn mark_stopped_at(&self, at: Instant) {
    let _ = self.stopped_at.set(at);
  }

  pub fn pause_at(&self, at: Instant) {
    if let Some(audio) = &self.audio {
      audio.pause();
    }
    if let Some(clock) = &self.audio_only_clock {
      clock.pause(at);
    }
    if let Some(commands) = &self.commands {
      let _ = commands.send(Command::Pause(at));
    }
    if let Some(camera) = &self.camera {
      let _ = camera.commands.send(Command::Pause(at));
    }
  }

  pub fn resume_at(&self, at: Instant) -> Result<(), String> {
    if let Some(audio) = &self.audio {
      audio.resume();
    }
    if let Some(clock) = &self.audio_only_clock {
      clock.resume(at);
    }
    self.commands.as_ref().map_or(Ok(()), |commands| {
      commands
        .send(Command::Resume(at))
        .map_err(|_| "The recording is no longer running".to_owned())
    })?;
    if let Some(camera) = &self.camera {
      camera
        .commands
        .send(Command::Resume(at))
        .map_err(|_| "The camera recording is no longer running".to_owned())?;
    }
    Ok(())
  }

  pub fn stop_at(mut self, at: Instant) -> Result<FinalizeInfo, String> {
    self.close_sources();
    let audio = self
      .audio
      .take()
      .map(audio::AudioCaptures::finish)
      .transpose()?;
    if let Some(clock) = self.audio_only_clock.take() {
      let audio = audio.ok_or_else(|| "The audio recording has no inputs".to_owned())?;
      let duration_ms = clock.duration_ms(at).max(1);
      let has_system_audio = audio.has_system_audio;
      let has_microphone = audio.has_microphone;
      let path = self
        .audio_only_path
        .take()
        .ok_or_else(|| "The audio recording path is unavailable".to_owned())?;
      audio::mux_audio_only(&path, duration_ms, audio)?;
      return Ok(FinalizeInfo {
        camera: None,
        cursor_path: None,
        duration_ms,
        has_microphone,
        has_system_audio,
        height: 0,
        path,
        poster: None,
        primary_kind: crate::recording::PrimaryRecordingKind::Audio,
        source_scale_factor: 1.0,
        width: 0,
      });
    }
    let (reply, replies) = mpsc::channel();
    self
      .commands
      .as_ref()
      .ok_or_else(|| "The recording writer is unavailable".to_owned())?
      .send(Command::Stop { at, reply })
      .map_err(|_| "The recording is no longer running".to_owned())?;
    let mut result = replies
      .recv_timeout(FINALIZE_TIMEOUT)
      .map_err(|_| "The recording did not finish in time".to_owned())?;
    self.join_writer();
    if result.is_ok() {
      if let Some(mut camera) = self.camera.take() {
        let (reply, replies) = mpsc::channel();
        let camera_result = camera
          .commands
          .send(Command::Stop { at, reply })
          .map_err(|_| "The camera recording is no longer running".to_owned())
          .and_then(|()| {
            replies
              .recv_timeout(FINALIZE_TIMEOUT)
              .map_err(|_| "The camera recording did not finish in time".to_owned())?
          });
        if let Some(worker) = camera.worker.take() {
          let _ = worker.join();
        }
        match camera_result {
          Ok(camera_info) => {
            if let Ok(info) = &mut result {
              info.camera = Some(super::encoding::CameraFinalizeInfo {
                duration_ms: camera_info.duration_ms,
                height: camera_info.height,
                path: camera_info.path,
                width: camera_info.width,
              });
            }
          }
          Err(error) => {
            eprintln!("Camera recording could not be finalized: {error}");
            let _ = std::fs::remove_file(&camera.path);
          }
        }
      }
    }
    if let (Ok(info), Some(audio)) = (&mut result, audio) {
      let has_system_audio = audio.has_system_audio;
      let has_microphone = audio.has_microphone;
      audio::mux(&info.path, info.duration_ms, audio)?;
      info.has_system_audio = has_system_audio;
      info.has_microphone = has_microphone;
    }
    result
  }

  pub fn cancel(mut self) {
    self.shutdown();
  }

  fn close_sources(&mut self) {
    if let Some(mut capture) = self.capture.take() {
      capture.close();
    }
    if let Some(camera) = self.primary_camera.take() {
      camera.stop();
    }
    if let Some(camera) = self.camera.as_mut() {
      if let Some(stream) = camera.stream.take() {
        stream.stop();
      }
    }
  }

  fn join_writer(&mut self) {
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
  }

  fn shutdown(&mut self) {
    self.close_sources();
    self.audio.take();
    if let Some(mut camera) = self.camera.take() {
      let _ = camera.commands.send(Command::Cancel);
      if let Some(worker) = camera.worker.take() {
        let _ = worker.join();
      }
      let _ = std::fs::remove_file(camera.path);
    }
    if let Some(commands) = &self.commands {
      let _ = commands.send(Command::Cancel);
    }
    self.join_writer();
  }
}

impl Drop for CaptureSession {
  fn drop(&mut self) {
    self.shutdown();
  }
}

pub fn begin_blocking(config: CaptureStartupConfig) -> Result<CaptureStart, String> {
  let CaptureStartupConfig {
    camera,
    camera_path,
    include_own_windows: _,
    microphone_id,
    monitor: recording_monitor,
    on_failure,
    path,
    primary,
    system_audio,
  } = config;
  let primary = match primary {
    PrimaryCaptureSource::Audio => {
      return begin_audio_only(
        microphone_id.as_deref(),
        &system_audio,
        recording_monitor,
        on_failure,
        path,
      );
    }
    primary => primary,
  };
  let camera_primary = matches!(primary, PrimaryCaptureSource::Camera);
  let mut camera_spec = camera.map(camera::CameraSpec::resolve).transpose()?;
  if camera_primary && camera_spec.is_none() {
    return Err("No camera is selected to record".to_owned());
  }
  let camera_selected = camera_spec.is_some();
  let mut graphics_source = None;
  let mut source_crop: Option<CaptureRect> = None;
  let (
    width,
    height,
    fps,
    primary_kind,
    source_scale_factor,
    cursor_source,
    wall_timestamped_frames,
  ) = match primary {
    PrimaryCaptureSource::Screen {
      fps,
      monitor_id,
      show_cursor,
    } => {
      let monitor = xcap::Monitor::all()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|monitor| monitor.id().ok() == Some(monitor_id))
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let source_scale_factor = monitor.scale_factor().map_err(|error| error.to_string())?;
      let monitor_x = monitor.x().map_err(|error| error.to_string())?;
      let monitor_y = monitor.y().map_err(|error| error.to_string())?;
      let width = monitor.width().map_err(|error| error.to_string())? & !1;
      let height = monitor.height().map_err(|error| error.to_string())? & !1;
      if width < 2 || height < 2 {
        return Err("The selected monitor has no recordable area".to_owned());
      }
      graphics_source = Some((
        CaptureTarget::Monitor(monitor_id),
        width,
        height,
        show_cursor,
      ));
      (
        width,
        height,
        fps,
        super::encoding::PrimaryRecordingKind::Screen,
        source_scale_factor,
        Some(CursorSource {
          height: f64::from(height),
          kind: CursorSourceKind::Screen,
          platform_id: monitor_id.to_string(),
          video_height: height,
          video_width: width,
          width: f64::from(width),
          x: f64::from(monitor_x),
          y: f64::from(monitor_y),
        }),
        false,
      )
    }
    PrimaryCaptureSource::Region {
      fps,
      monitor_id,
      region,
      show_cursor,
    } => {
      let monitor = xcap::Monitor::all()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|monitor| monitor.id().ok() == Some(monitor_id))
        .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
      let source_scale_factor = monitor.scale_factor().map_err(|error| error.to_string())?;
      let monitor_x = monitor.x().map_err(|error| error.to_string())?;
      let monitor_y = monitor.y().map_err(|error| error.to_string())?;
      let monitor_width = monitor.width().map_err(|error| error.to_string())?;
      let monitor_height = monitor.height().map_err(|error| error.to_string())?;
      let crop = physical_capture_rect(
        region,
        f64::from(source_scale_factor),
        monitor_width,
        monitor_height,
      )
      .and_then(video_capture_rect)
      .ok_or_else(|| "The selected region is too small or outside the monitor".to_owned())?;
      source_crop = Some(crop);
      graphics_source = Some((
        CaptureTarget::Monitor(monitor_id),
        monitor_width,
        monitor_height,
        show_cursor,
      ));
      (
        crop.width,
        crop.height,
        fps,
        super::encoding::PrimaryRecordingKind::Screen,
        source_scale_factor,
        Some(CursorSource {
          height: f64::from(crop.height),
          kind: CursorSourceKind::Region,
          platform_id: monitor_id.to_string(),
          video_height: crop.height,
          video_width: crop.width,
          width: f64::from(crop.width),
          x: f64::from(monitor_x) + f64::from(crop.x),
          y: f64::from(monitor_y) + f64::from(crop.y),
        }),
        true,
      )
    }
    PrimaryCaptureSource::Window {
      fps,
      show_cursor,
      window_id,
    } => {
      let window = xcap::Window::all()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|window| window.id().ok() == Some(window_id))
        .ok_or_else(|| "The selected window is no longer available".to_owned())?;
      if window.is_minimized().unwrap_or(true) {
        return Err("The selected window is minimized".to_owned());
      }
      let target = CaptureTarget::Window(window_id);
      let (width, height) = capture::target_size(target)?;
      let window_x = window.x().map_err(|error| error.to_string())?;
      let window_y = window.y().map_err(|error| error.to_string())?;
      let window_width = window.width().map_err(|error| error.to_string())?;
      let window_height = window.height().map_err(|error| error.to_string())?;
      let source_scale_factor = window
        .current_monitor()
        .and_then(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);
      graphics_source = Some((target, width, height, show_cursor));
      (
        width,
        height,
        fps,
        super::encoding::PrimaryRecordingKind::Screen,
        source_scale_factor,
        Some(CursorSource {
          height: f64::from(window_height),
          kind: CursorSourceKind::Window,
          platform_id: window_id.to_string(),
          video_height: height,
          video_width: width,
          width: f64::from(window_width),
          x: f64::from(window_x),
          y: f64::from(window_y),
        }),
        true,
      )
    }
    PrimaryCaptureSource::Camera => {
      let camera = camera_spec.as_ref().expect("checked above");
      (
        camera.width,
        camera.height,
        camera.fps,
        super::encoding::PrimaryRecordingKind::Camera,
        1.0,
        None,
        false,
      )
    }
    _ => return Err("This recording source is not yet available on Windows".to_owned()),
  };

  let timeline_origin = Arc::new(OnceLock::new());
  let audio = audio::AudioCaptures::start(
    microphone_id.as_deref(),
    &system_audio,
    Arc::clone(&timeline_origin),
    Arc::clone(&recording_monitor),
    Arc::clone(&on_failure),
    &path,
  )?;
  let device = capture::create_device()?;
  let stopped_at = Arc::new(OnceLock::new());
  let (commands, first_frame, worker) = spawn_writer(
    "screenwide-windows-recording-writer",
    WriterConfig {
      device: device.clone(),
      establish_timeline_origin: !camera_selected,
      fps,
      height,
      on_failure: Arc::clone(&on_failure),
      path,
      primary_kind,
      source_crop,
      stopped_at: Arc::clone(&stopped_at),
      timeline_origin: Arc::clone(&timeline_origin),
      wall_timestamped_frames,
      width,
    },
  )?;
  let mut session = CaptureSession {
    audio: Some(audio),
    audio_only_clock: None,
    audio_only_path: None,
    camera: None,
    capture: None,
    commands: Some(commands.clone()),
    primary_camera: None,
    stopped_at,
    worker: Some(worker),
  };
  let mut camera_first_frame = None;
  if !camera_primary {
    if let Some(spec) = camera_spec.take() {
      let camera_path = camera_path.ok_or_else(|| "The camera has nowhere to record".to_owned())?;
      let (camera_commands, camera_ready, camera_worker) = spawn_writer(
        "screenwide-windows-camera-writer",
        WriterConfig {
          device: device.clone(),
          establish_timeline_origin: false,
          fps: spec.fps,
          height: spec.height,
          on_failure: Arc::clone(&on_failure),
          path: camera_path.clone(),
          primary_kind: super::encoding::PrimaryRecordingKind::Camera,
          source_crop: None,
          stopped_at: Arc::clone(&session.stopped_at),
          timeline_origin: Arc::clone(&timeline_origin),
          wall_timestamped_frames: false,
          width: spec.width,
        },
      )?;
      let stream = camera::start(
        spec,
        device.clone(),
        camera_commands.clone(),
        Arc::clone(&timeline_origin),
        Arc::clone(&recording_monitor),
        Arc::clone(&on_failure),
      )?;
      camera_first_frame = Some(camera_ready);
      session.camera = Some(CameraRecording {
        commands: camera_commands,
        path: camera_path,
        stream: Some(stream),
        worker: Some(camera_worker),
      });
    }
  }
  if let Some((target, capture_width, capture_height, show_cursor)) = graphics_source {
    // Match the user's capture choice exactly. Cursor metadata remains a
    // separate editable layer even when native pixels include the pointer.
    session.capture = Some(CaptureObjects::start(
      device,
      target,
      capture_width,
      capture_height,
      show_cursor,
      commands,
    )?);
  } else {
    let spec = camera_spec.expect("camera-primary checked above");
    session.primary_camera = Some(camera::start(
      spec,
      device,
      commands,
      Arc::clone(&timeline_origin),
      recording_monitor,
      on_failure,
    )?);
  }
  let first_frame = match camera_first_frame {
    Some(camera) => both_first_frames(first_frame, camera),
    None => first_frame,
  };

  Ok(CaptureStart {
    cursor_source,
    first_frame,
    session,
    source_scale_factor,
    timeline_origin,
  })
}

type WriterSpawn = (
  mpsc::SyncSender<Command>,
  mpsc::Receiver<Result<(), String>>,
  JoinHandle<()>,
);

fn spawn_writer(name: &str, config: WriterConfig) -> Result<WriterSpawn, String> {
  let (commands, command_rx) = mpsc::sync_channel(8);
  let (initialized_tx, initialized) = mpsc::channel();
  let (first_frame_tx, first_frame) = mpsc::channel();
  let worker = std::thread::Builder::new()
    .name(name.to_owned())
    .spawn(move || writer::run(config, command_rx, initialized_tx, first_frame_tx))
    .map_err(|error| error.to_string())?;
  initialized
    .recv()
    .map_err(|_| "The recording writer stopped during startup".to_owned())??;
  Ok((commands, first_frame, worker))
}

fn both_first_frames(
  primary: mpsc::Receiver<Result<(), String>>,
  camera: mpsc::Receiver<Result<(), String>>,
) -> mpsc::Receiver<Result<(), String>> {
  let (ready, combined) = mpsc::channel();
  std::thread::spawn(move || {
    let result = primary
      .recv()
      .map_err(|_| "The primary recording stopped before its first frame".to_owned())
      .and_then(|result| result)
      .and_then(|()| {
        camera
          .recv()
          .map_err(|_| "The camera stopped before its first frame".to_owned())?
      });
    let _ = ready.send(result);
  });
  combined
}

fn begin_audio_only(
  microphone_id: Option<&str>,
  system_audio: &crate::recording::SystemAudioSelection,
  monitor: Arc<crate::recording::monitor::RecordingMonitor>,
  on_failure: crate::recording::encoding::FailureReport,
  path: std::path::PathBuf,
) -> Result<CaptureStart, String> {
  if microphone_id.is_none() && !system_audio.enabled {
    return Err("Select a microphone or system audio source".to_owned());
  }
  let started = Instant::now();
  let timeline_origin = Arc::new(OnceLock::new());
  let _ = timeline_origin.set(started);
  let audio = audio::AudioCaptures::start(
    microphone_id,
    system_audio,
    Arc::clone(&timeline_origin),
    monitor,
    on_failure,
    &path,
  )?;
  let (ready, first_frame) = mpsc::channel();
  let _ = ready.send(Ok(()));
  Ok(CaptureStart {
    cursor_source: None,
    first_frame,
    session: CaptureSession {
      audio: Some(audio),
      audio_only_clock: Some(AudioOnlyClock::new(started)),
      audio_only_path: Some(path),
      camera: None,
      capture: None,
      commands: None,
      primary_camera: None,
      stopped_at: Arc::new(OnceLock::new()),
      worker: None,
    },
    source_scale_factor: 1.0,
    timeline_origin,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::recording::{monitor::RecordingMonitor, CameraCaptureMode, SystemAudioSelection};

  #[test]
  #[ignore = "requires an interactive Windows camera and hardware encoder"]
  fn records_a_playable_camera_sample() {
    let info = nokhwa::query(nokhwa::utils::ApiBackend::Auto)
      .unwrap()
      .into_iter()
      .next()
      .expect("connect a camera before running this test");
    let format = crate::camera_format::available_camera_formats(info.index(), 30)
      .unwrap()
      .into_iter()
      .next()
      .expect("the camera has no supported recording mode");
    let resolution = format.resolution();
    let path = std::env::temp_dir().join(format!(
      "screenwide-windows-camera-{}.mp4",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let start = begin_blocking(CaptureStartupConfig {
      camera: Some(CameraCaptureMode {
        device_id: crate::recording_inputs::camera_id(&info),
        flipped: false,
        fps: format.frame_rate(),
        height: resolution.height(),
        width: resolution.width(),
      }),
      camera_path: None,
      include_own_windows: true,
      microphone_id: None,
      monitor: Arc::new(RecordingMonitor::default()),
      on_failure: Arc::new(|error| eprintln!("camera recording failure: {error}")),
      path: path.clone(),
      primary: PrimaryCaptureSource::Camera,
      system_audio: SystemAudioSelection::default(),
    })
    .unwrap();
    start
      .first_frame
      .recv_timeout(Duration::from_secs(10))
      .unwrap()
      .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let stopped_at = Instant::now();
    start.session.mark_stopped_at(stopped_at);
    let info = start.session.stop_at(stopped_at).unwrap();
    assert_eq!(
      info.primary_kind,
      super::super::encoding::PrimaryRecordingKind::Camera
    );
    assert_eq!(
      (info.width, info.height),
      (resolution.width(), resolution.height())
    );
    assert!(
      info.duration_ms >= 1_500,
      "duration was {} ms",
      info.duration_ms
    );
    assert!(std::fs::metadata(&path).unwrap().len() > 1_024);
    std::fs::remove_file(path).unwrap();
  }

  #[test]
  #[ignore = "requires an interactive Windows display, camera, and hardware encoders"]
  fn records_synchronized_screen_and_camera_samples() {
    let monitor = xcap::Monitor::all().unwrap().into_iter().next().unwrap();
    let monitor_id = monitor.id().unwrap();
    let camera = nokhwa::query(nokhwa::utils::ApiBackend::Auto)
      .unwrap()
      .into_iter()
      .next()
      .expect("connect a camera before running this test");
    let format = crate::camera_format::available_camera_formats(camera.index(), 30)
      .unwrap()
      .into_iter()
      .next()
      .expect("the camera has no supported recording mode");
    let resolution = format.resolution();
    let directory = std::env::temp_dir();
    let screen_path = directory.join(format!(
      "screenwide-windows-screen-camera-{}.mp4",
      std::process::id()
    ));
    let camera_path = directory.join(format!(
      "screenwide-windows-camera-sidecar-{}.mp4",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&screen_path);
    let _ = std::fs::remove_file(&camera_path);
    let start = begin_blocking(CaptureStartupConfig {
      camera: Some(CameraCaptureMode {
        device_id: crate::recording_inputs::camera_id(&camera),
        flipped: false,
        fps: format.frame_rate(),
        height: resolution.height(),
        width: resolution.width(),
      }),
      camera_path: Some(camera_path.clone()),
      include_own_windows: true,
      microphone_id: None,
      monitor: Arc::new(RecordingMonitor::default()),
      on_failure: Arc::new(|error| eprintln!("screen/camera recording failure: {error}")),
      path: screen_path.clone(),
      primary: PrimaryCaptureSource::Screen {
        fps: 60,
        monitor_id,
        show_cursor: false,
      },
      system_audio: SystemAudioSelection::default(),
    })
    .unwrap();
    start
      .first_frame
      .recv_timeout(Duration::from_secs(10))
      .unwrap()
      .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let stopped_at = Instant::now();
    start.session.mark_stopped_at(stopped_at);
    let info = start.session.stop_at(stopped_at).unwrap();
    let camera_info = info.camera.expect("camera sidecar was not finalized");
    assert_eq!(
      (camera_info.width, camera_info.height),
      (resolution.width(), resolution.height())
    );
    assert!(
      info.duration_ms.abs_diff(camera_info.duration_ms) <= 100,
      "screen was {} ms but camera was {} ms",
      info.duration_ms,
      camera_info.duration_ms
    );
    assert!(std::fs::metadata(&screen_path).unwrap().len() > 1_024);
    assert!(std::fs::metadata(&camera_path).unwrap().len() > 1_024);
    std::fs::remove_file(screen_path).unwrap();
    std::fs::remove_file(camera_path).unwrap();
  }

  #[test]
  #[ignore = "requires an interactive Windows display and hardware encoder"]
  fn records_a_playable_screen_sample() {
    let monitor = xcap::Monitor::all().unwrap().into_iter().next().unwrap();
    let monitor_id = monitor.id().unwrap();
    let path = std::env::temp_dir().join(format!(
      "screenwide-windows-recording-{}.mp4",
      std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let start = begin_blocking(CaptureStartupConfig {
      camera: None,
      camera_path: None,
      include_own_windows: true,
      microphone_id: None,
      monitor: Arc::new(RecordingMonitor::default()),
      on_failure: Arc::new(|error| eprintln!("recording failure: {error}")),
      path: path.clone(),
      primary: PrimaryCaptureSource::Screen {
        fps: 60,
        monitor_id,
        show_cursor: true,
      },
      system_audio: SystemAudioSelection::default(),
    })
    .unwrap();
    start
      .first_frame
      .recv_timeout(Duration::from_secs(5))
      .unwrap()
      .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let stopped_at = Instant::now();
    start.session.mark_stopped_at(stopped_at);
    // Reproduce a busy async finalizer: frames may keep arriving during this
    // delay, but none may extend the recording past the user's stop instant.
    std::thread::sleep(Duration::from_secs(3));
    let info = start.session.stop_at(stopped_at).unwrap();
    assert!(
      info.duration_ms >= 900,
      "duration was {} ms",
      info.duration_ms
    );
    assert!(
      info.duration_ms <= 1_500,
      "stop finalization added a frozen tail: {} ms",
      info.duration_ms
    );
    assert!(std::fs::metadata(&path).unwrap().len() > 1_024);
    std::fs::remove_file(path).unwrap();
  }
}
