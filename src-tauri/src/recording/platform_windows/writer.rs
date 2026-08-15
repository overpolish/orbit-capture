// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::{mpsc, Arc, OnceLock};
use std::time::Instant;

use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::recording::encoding::{bitrate_bps, FailureReport, FinalizeInfo, Timeline};
use crate::recording::PrimaryRecordingKind;

const NANOS_PER_100NS: i64 = 100;

fn win<T>(result: windows::core::Result<T>) -> Result<T, String> {
  result.map_err(|error| error.to_string())
}

pub(super) struct Frame {
  pub(super) source_100ns: i64,
  pub(super) texture: ID3D11Texture2D,
  pub(super) wall: Instant,
}

pub(super) enum Command {
  Frame(Frame),
  Pause(Instant),
  Resume(Instant),
  Stop {
    at: Instant,
    reply: mpsc::Sender<Result<FinalizeInfo, String>>,
  },
  Cancel,
}

pub(super) struct WriterConfig {
  pub(super) device: ID3D11Device,
  pub(super) fps: u32,
  pub(super) height: u32,
  pub(super) on_failure: FailureReport,
  pub(super) path: PathBuf,
  pub(super) primary_kind: PrimaryRecordingKind,
  pub(super) establish_timeline_origin: bool,
  pub(super) stopped_at: Arc<OnceLock<Instant>>,
  pub(super) timeline_origin: Arc<OnceLock<Instant>>,
  pub(super) width: u32,
}

struct MediaFoundation;

impl MediaFoundation {
  fn start() -> Result<Self, String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
      .ok()
      .map_err(|error| error.to_string())?;
    if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
      unsafe { CoUninitialize() };
      return Err(error.to_string());
    }
    Ok(Self)
  }
}

impl Drop for MediaFoundation {
  fn drop(&mut self) {
    let _ = unsafe { MFShutdown() };
    unsafe { CoUninitialize() };
  }
}

struct Sink {
  _byte_stream: IMFByteStream,
  _device_manager: IMFDXGIDeviceManager,
  media_sink: IMFMediaSink,
  sink: Option<IMFSinkWriter>,
  stream: u32,
}

impl Sink {
  fn new(config: &WriterConfig) -> Result<Self, String> {
    let mut reset_token = 0;
    let mut manager = None;
    unsafe { MFCreateDXGIDeviceManager(&mut reset_token, &mut manager) }
      .map_err(|error| error.to_string())?;
    let manager = manager.ok_or_else(|| "Media Foundation created no D3D manager".to_owned())?;
    unsafe { manager.ResetDevice(&config.device, reset_token) }
      .map_err(|error| error.to_string())?;

    let attributes = attributes(4)?;
    win(unsafe {
      attributes.SetGUID(
        &MF_TRANSCODE_CONTAINERTYPE,
        &MFTranscodeContainerType_FMPEG4,
      )
    })?;
    win(unsafe { attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1) })?;
    win(unsafe { attributes.SetUINT32(&MF_LOW_LATENCY, 1) })?;
    win(unsafe { attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager) })?;
    let path = config
      .path
      .to_str()
      .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
    let wide = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let output = video_type(MFVideoFormat_H264, config.width, config.height, config.fps)?;
    win(unsafe {
      output.SetUINT32(
        &MF_MT_AVG_BITRATE,
        u32::try_from(bitrate_bps(config.width, config.height, config.fps)).unwrap_or(u32::MAX),
      )
    })?;
    win(unsafe { output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32) })?;
    win(unsafe { output.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, (config.fps / 2).max(1)) })?;
    let byte_stream = win(unsafe {
      MFCreateFile(
        MF_ACCESSMODE_WRITE,
        MF_OPENMODE_DELETE_IF_EXIST,
        MF_FILEFLAGS_NONE,
        PCWSTR(wide.as_ptr()),
      )
    })?;
    let media_sink = win(unsafe { MFCreateFMPEG4MediaSink(&byte_stream, &output, None) })?;
    let sink_attributes = media_sink
      .cast::<IMFAttributes>()
      .map_err(|error| error.to_string())?;
    // Half-second fragments align with the working encoder's half-second GOP,
    // bounding crash loss while keeping seeks cheap in the export scrubber.
    win(unsafe { sink_attributes.SetUINT64(&MF_MPEG4SINK_MIN_FRAGMENT_DURATION, 5_000_000) })?;
    let sink = win(unsafe { MFCreateSinkWriterFromMediaSink(&media_sink, &attributes) })?;
    // MFCreateFMPEG4MediaSink has already created the fixed video stream. The
    // Sink Writer must feed that stream so its encoder can update the sink's
    // media type with the generated H.264 sequence header.
    let stream = 0;

    let input = video_type(
      MFVideoFormat_ARGB32,
      config.width,
      config.height,
      config.fps,
    )?;
    win(unsafe { sink.SetInputMediaType(stream, &input, None) })?;
    win(unsafe { sink.BeginWriting() })?;

    Ok(Self {
      _byte_stream: byte_stream,
      _device_manager: manager,
      media_sink,
      sink: Some(sink),
      stream,
    })
  }

  fn write(&self, frame: &Frame, pts_100ns: i64, duration_100ns: i64) -> Result<(), String> {
    let buffer =
      unsafe { MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &frame.texture, 0, false) }
        .map_err(|error| error.to_string())?;
    // A DXGI surface buffer starts with a current length of zero. Sink Writer
    // treats that as an empty video sample even though the texture itself is
    // valid, and the H.264 transform rejects it with E_INVALIDARG. Preserve
    // the GPU surface and describe its payload through IMF2DBuffer; this does
    // not copy or map the frame back to the CPU.
    let two_dimensional = buffer
      .cast::<IMF2DBuffer>()
      .map_err(|error| error.to_string())?;
    let length =
      unsafe { two_dimensional.GetContiguousLength() }.map_err(|error| error.to_string())?;
    win(unsafe { buffer.SetCurrentLength(length) })?;
    let sample = unsafe { MFCreateSample() }.map_err(|error| error.to_string())?;
    win(unsafe { sample.AddBuffer(&buffer) })?;
    win(unsafe { sample.SetSampleTime(pts_100ns) })?;
    win(unsafe { sample.SetSampleDuration(duration_100ns.max(1)) })?;
    let sink = self
      .sink
      .as_ref()
      .ok_or_else(|| "The recording has already been finalized".to_owned())?;
    win(unsafe { sink.WriteSample(self.stream, &sample) })?;
    Ok(())
  }

  fn finish(&mut self) -> Result<(), String> {
    let sink = self
      .sink
      .take()
      .ok_or_else(|| "The recording has already been finalized".to_owned())?;
    win(unsafe { sink.Finalize() })?;
    drop(sink);
    win(unsafe { self.media_sink.Shutdown() })
  }
}

impl Drop for Sink {
  fn drop(&mut self) {
    self.sink.take();
    let _ = unsafe { self.media_sink.Shutdown() };
  }
}

fn attributes(capacity: u32) -> Result<IMFAttributes, String> {
  let mut value = None;
  unsafe { MFCreateAttributes(&mut value, capacity) }.map_err(|error| error.to_string())?;
  value.ok_or_else(|| "Media Foundation created no attributes".to_owned())
}

fn video_type(
  subtype: windows::core::GUID,
  width: u32,
  height: u32,
  fps: u32,
) -> Result<IMFMediaType, String> {
  let media_type = unsafe { MFCreateMediaType() }.map_err(|error| error.to_string())?;
  let packed_size = (u64::from(width) << 32) | u64::from(height);
  let packed_rate = (u64::from(fps) << 32) | 1;
  let square_pixels = (1_u64 << 32) | 1;
  win(unsafe { media_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })?;
  win(unsafe { media_type.SetGUID(&MF_MT_SUBTYPE, &subtype) })?;
  win(unsafe { media_type.SetUINT64(&MF_MT_FRAME_SIZE, packed_size) })?;
  win(unsafe { media_type.SetUINT64(&MF_MT_FRAME_RATE, packed_rate) })?;
  win(unsafe { media_type.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, square_pixels) })?;
  win(unsafe {
    media_type.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
  })?;
  Ok(media_type)
}

struct Writer {
  base: Instant,
  config: WriterConfig,
  failed: Option<String>,
  frame_duration_100ns: i64,
  last_appended_ns: Option<i64>,
  sink: Sink,
  tail: Option<Frame>,
  timeline: Timeline,
}

impl Writer {
  fn new(config: WriterConfig) -> Result<Self, String> {
    let sink = Sink::new(&config)?;
    Ok(Self {
      base: Instant::now(),
      frame_duration_100ns: 10_000_000_i64 / i64::from(config.fps.max(1)),
      config,
      failed: None,
      last_appended_ns: None,
      sink,
      tail: None,
      timeline: Timeline::default(),
    })
  }

  fn elapsed_ns(&self, at: Instant) -> i64 {
    i64::try_from(at.saturating_duration_since(self.base).as_nanos()).unwrap_or(i64::MAX)
  }

  fn append(&mut self, frame: &Frame, pts_ns: i64, duration_100ns: i64) -> bool {
    if self.failed.is_some() {
      return false;
    }
    match self
      .sink
      .write(frame, pts_ns / NANOS_PER_100NS, duration_100ns)
    {
      Ok(()) => {
        self.last_appended_ns = Some(pts_ns);
        true
      }
      Err(error) => {
        let reason = format!("Media Foundation stopped accepting video frames: {error}");
        (self.config.on_failure)(reason.clone());
        self.failed = Some(reason);
        false
      }
    }
  }

  fn frame(&mut self, frame: Frame) -> bool {
    if after_stop(&self.config.stopped_at, frame.wall) {
      return false;
    }
    if self.timeline.is_paused() {
      self.tail = Some(frame);
      return false;
    }
    let is_first = !self.timeline.has_started();
    let source_ns = frame.source_100ns.saturating_mul(NANOS_PER_100NS);
    if is_first {
      if self.config.establish_timeline_origin {
        let _ = self.config.timeline_origin.set(frame.wall);
      }
      let Some(origin) = self.config.timeline_origin.get().copied() else {
        // A secondary camera can become ready before the primary screen. Its
        // warm frames are deliberately discarded until the primary track
        // establishes the shared zero used by preview and export.
        self.tail = Some(frame);
        return false;
      };
      let offset_ns =
        i64::try_from(frame.wall.saturating_duration_since(origin).as_nanos()).unwrap_or(i64::MAX);
      self
        .timeline
        .start_at(source_ns.saturating_sub(offset_ns), self.elapsed_ns(origin));
    }
    let wall_ns = self.elapsed_ns(frame.wall);
    let pts_ns = self.timeline.frame_pts_ns(source_ns, wall_ns);
    let appended = self.append(&frame, pts_ns, self.frame_duration_100ns);
    self.tail = Some(frame);
    is_first && appended
  }

  fn finish(&mut self, at: Instant) -> Result<FinalizeInfo, String> {
    if !self.timeline.has_started() {
      return Err("The recording captured no frames".to_owned());
    }
    let stop_ns = self.timeline.stop_pts_ns(self.elapsed_ns(at));
    if let Some(tail) = self.tail.take() {
      self.append(&tail, stop_ns, 1);
      self.tail = Some(tail);
    }
    if let Some(error) = self.failed.take() {
      return Err(error);
    }
    self.sink.finish()?;
    let end_ns = self.last_appended_ns.unwrap_or_default();
    Ok(FinalizeInfo {
      camera: None,
      cursor_path: None,
      duration_ms: u64::try_from(end_ns / 1_000_000).unwrap_or_default(),
      has_microphone: false,
      has_system_audio: false,
      height: self.config.height,
      path: self.config.path.clone(),
      poster: None,
      primary_kind: self.config.primary_kind,
      source_scale_factor: 1.0,
      width: self.config.width,
    })
  }
}

fn after_stop(stopped_at: &OnceLock<Instant>, frame_at: Instant) -> bool {
  stopped_at
    .get()
    .is_some_and(|stopped_at| frame_at > *stopped_at)
}

pub(super) fn run(
  config: WriterConfig,
  commands: mpsc::Receiver<Command>,
  first_frame: mpsc::Sender<Result<(), String>>,
) {
  let _media_foundation = match MediaFoundation::start() {
    Ok(runtime) => runtime,
    Err(error) => {
      let _ = first_frame.send(Err(error));
      return;
    }
  };
  let mut writer = match Writer::new(config) {
    Ok(writer) => writer,
    Err(error) => {
      let _ = first_frame.send(Err(error));
      return;
    }
  };
  let mut announced = false;
  while let Ok(command) = commands.recv() {
    match command {
      Command::Frame(frame) => {
        if writer.frame(frame) && !announced {
          announced = true;
          let _ = first_frame.send(Ok(()));
        }
      }
      Command::Pause(at) => {
        let elapsed = writer.elapsed_ns(at);
        writer.timeline.pause(elapsed);
      }
      Command::Resume(at) => {
        let elapsed = writer.elapsed_ns(at);
        writer.timeline.resume(elapsed);
      }
      Command::Stop { at, reply } => {
        let _ = reply.send(writer.finish(at));
        return;
      }
      Command::Cancel => return,
    }
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  #[test]
  fn rejects_frames_captured_after_the_user_pressed_stop() {
    let base = Instant::now();
    let stopped_at = OnceLock::new();
    assert!(!after_stop(&stopped_at, base + Duration::from_secs(10)));
    stopped_at.set(base + Duration::from_secs(1)).unwrap();
    assert!(!after_stop(&stopped_at, base + Duration::from_secs(1)));
    assert!(after_stop(
      &stopped_at,
      base + Duration::from_secs(1) + Duration::from_nanos(1)
    ));
  }
}
