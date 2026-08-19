// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Graphics::Direct3D11::{
  ID3D11Device, ID3D11Resource, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
  D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{
  CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Variant::VT_UI4;
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PSCreateMemoryPropertyStore};

use crate::capture_geometry::CaptureRect;
use crate::recording::encoding::{bitrate_bps, FailureReport, FinalizeInfo, Timeline};
use crate::recording::PrimaryRecordingKind;

const NANOS_PER_100NS: i64 = 100;

fn frame_cadence(fps: u32) -> Duration {
  Duration::from_nanos(1_000_000_000_u64 / u64::from(fps.max(1)))
}

fn win<T>(result: windows::core::Result<T>) -> Result<T, String> {
  result.map_err(|error| error.to_string())
}

/// Takes ownership of window content before WGC can recycle its frame-pool
/// surface. The copy never leaves the GPU; Media Foundation may safely retain
/// samples that refer to this immutable texture while a later WGC frame is
/// being cached.
fn snapshot_frame(device: &ID3D11Device, mut frame: Frame) -> Result<Frame, String> {
  let mut source_description = D3D11_TEXTURE2D_DESC::default();
  unsafe { frame.texture.GetDesc(&mut source_description) };
  let description = D3D11_TEXTURE2D_DESC {
    Usage: D3D11_USAGE_DEFAULT,
    BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
    CPUAccessFlags: 0,
    MiscFlags: 0,
    ..source_description
  };
  let mut texture = None;
  unsafe { device.CreateTexture2D(&description, None, Some(&mut texture)) }
    .map_err(|error| error.to_string())?;
  let texture = texture.ok_or_else(|| "Direct3D created no cached window frame".to_owned())?;
  let context = unsafe { device.GetImmediateContext() }.map_err(|error| error.to_string())?;
  let source = frame
    .texture
    .cast::<ID3D11Resource>()
    .map_err(|error| error.to_string())?;
  let target = texture
    .cast::<ID3D11Resource>()
    .map_err(|error| error.to_string())?;
  unsafe { context.CopyResource(&target, &source) };
  frame.texture = texture;
  Ok(frame)
}

/// Copies a monitor-local region into an encoder-sized texture without
/// mapping either surface to the CPU.
fn crop_frame(device: &ID3D11Device, mut frame: Frame, crop: CaptureRect) -> Result<Frame, String> {
  let mut source_description = D3D11_TEXTURE2D_DESC::default();
  unsafe { frame.texture.GetDesc(&mut source_description) };
  let description = D3D11_TEXTURE2D_DESC {
    Width: crop.width,
    Height: crop.height,
    Usage: D3D11_USAGE_DEFAULT,
    BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
    CPUAccessFlags: 0,
    MiscFlags: 0,
    ..source_description
  };
  let mut texture = None;
  unsafe { device.CreateTexture2D(&description, None, Some(&mut texture)) }
    .map_err(|error| error.to_string())?;
  let texture = texture.ok_or_else(|| "Direct3D created no cropped region frame".to_owned())?;
  let context = unsafe { device.GetImmediateContext() }.map_err(|error| error.to_string())?;
  let source = frame
    .texture
    .cast::<ID3D11Resource>()
    .map_err(|error| error.to_string())?;
  let target = texture
    .cast::<ID3D11Resource>()
    .map_err(|error| error.to_string())?;
  let source_box = D3D11_BOX {
    left: crop.x,
    top: crop.y,
    front: 0,
    right: crop.x.saturating_add(crop.width),
    bottom: crop.y.saturating_add(crop.height),
    back: 1,
  };
  unsafe {
    context.CopySubresourceRegion(&target, 0, 0, 0, 0, &source, 0, Some(&source_box));
  }
  frame.texture = texture;
  Ok(frame)
}

#[derive(Clone)]
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
  pub(super) source_crop: Option<CaptureRect>,
  pub(super) establish_timeline_origin: bool,
  pub(super) stopped_at: Arc<OnceLock<Instant>>,
  pub(super) timeline_origin: Arc<OnceLock<Instant>>,
  pub(super) wall_timestamped_frames: bool,
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

    let attributes = attributes(5)?;
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
    // MF_MT_MAX_KEYFRAME_SPACING above is advisory and the Microsoft AVC DX12
    // Encoder HMFT (Windows 11 D3D12 shim) ignores it, as it does ICodecAPI
    // calls made after SetInputMediaType and per-sample IDR requests. Codec
    // API values handed over as MF_SINK_WRITER_ENCODER_CONFIG reach the
    // encoder before its media types are negotiated and are honoured, giving
    // working recordings the half-second GOP the export scrubber relies on.
    match encoder_config((config.fps / 2).max(1)) {
      Ok(store) => win(unsafe { attributes.SetUnknown(&MF_SINK_WRITER_ENCODER_CONFIG, &store) })?,
      Err(message) => eprintln!("windows recorder: encoder config not applied: {message}"),
    }
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
    unsafe { sink.WriteSample(self.stream, &sample) }.map_err(|error| {
      let mut description = D3D11_TEXTURE2D_DESC::default();
      unsafe { frame.texture.GetDesc(&mut description) };
      format!(
        "{error} (sample time {pts_100ns} / duration {duration_100ns} x100ns; texture {}x{} format {:?} bind 0x{:x} misc 0x{:x} usage {:?})",
        description.Width,
        description.Height,
        description.Format,
        description.BindFlags,
        description.MiscFlags,
        description.Usage,
      )
    })?;
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

fn encoder_config(gop_frames: u32) -> Result<IPropertyStore, String> {
  let mut raw = std::ptr::null_mut();
  win(unsafe { PSCreateMemoryPropertyStore(&IPropertyStore::IID, &mut raw) })?;
  let store = unsafe { IPropertyStore::from_raw(raw) };
  let mut value = PROPVARIANT::default();
  unsafe {
    (*value.Anonymous.Anonymous).vt = VT_UI4;
    (*value.Anonymous.Anonymous).Anonymous.ulVal = gop_frames;
  }
  let key = PROPERTYKEY {
    fmtid: CODECAPI_AVEncMPVGOPSize,
    pid: 0,
  };
  win(unsafe { store.SetValue(&key, &value) })?;
  Ok(store)
}

/// Best-effort identification of the H.264 transform the sink writer chose,
/// so GOP behaviour can be tied to a vendor when a recording misbehaves.
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
    let frame = if let Some(crop) = self.config.source_crop {
      match crop_frame(&self.config.device, frame, crop) {
        Ok(frame) => frame,
        Err(error) => {
          let reason = format!("Direct3D could not crop the region frame: {error}");
          (self.config.on_failure)(reason.clone());
          self.failed = Some(reason);
          return false;
        }
      }
    } else if self.config.wall_timestamped_frames {
      match snapshot_frame(&self.config.device, frame) {
        Ok(frame) => frame,
        Err(error) => {
          let reason = format!("Direct3D could not cache the window frame: {error}");
          (self.config.on_failure)(reason.clone());
          self.failed = Some(reason);
          return false;
        }
      }
    } else {
      frame
    };
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
    if self.config.wall_timestamped_frames && !is_first {
      self.tail = Some(frame);
      return false;
    }
    let wall_ns = self.elapsed_ns(frame.wall);
    let pts_ns = if self.config.wall_timestamped_frames {
      self.timeline.wall_frame_pts_ns(wall_ns)
    } else {
      self.timeline.frame_pts_ns(source_ns, wall_ns)
    };
    let appended = self.append(&frame, pts_ns, self.frame_duration_100ns);
    self.tail = Some(frame);
    is_first && appended
  }

  fn tick(&mut self, at: Instant) {
    if self.timeline.is_paused() || !self.timeline.has_started() {
      return;
    }
    let Some(frame) = self.tail.clone() else {
      return;
    };
    if after_stop(&self.config.stopped_at, at) {
      return;
    }
    let wall_ns = self.elapsed_ns(at);
    let pts_ns = self.timeline.wall_frame_pts_ns(wall_ns);
    self.append(&frame, pts_ns, self.frame_duration_100ns);
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
  initialized: mpsc::Sender<Result<(), String>>,
  first_frame: mpsc::Sender<Result<(), String>>,
) {
  let _media_foundation = match MediaFoundation::start() {
    Ok(runtime) => runtime,
    Err(error) => {
      let _ = initialized.send(Err(error.clone()));
      let _ = first_frame.send(Err(error));
      return;
    }
  };
  let mut writer = match Writer::new(config) {
    Ok(writer) => writer,
    Err(error) => {
      let _ = initialized.send(Err(error.clone()));
      let _ = first_frame.send(Err(error));
      return;
    }
  };
  let _ = initialized.send(Ok(()));
  let mut announced = false;
  let cadence = frame_cadence(writer.config.fps);
  let mut next_tick: Option<Instant> = None;
  let mut pending = None;
  loop {
    // A continuously changing window can keep the frame channel readable at
    // all times. Honour an elapsed presentation deadline before reading more
    // capture work so incoming WGC frames cannot starve the fixed-rate clock.
    if let Some(deadline) = next_tick {
      if Instant::now() >= deadline {
        writer.tick(deadline);
        next_tick = Some(deadline + cadence);
        continue;
      }
    }
    let command = match pending.take() {
      Some(command) => Some(command),
      None => match next_tick {
        Some(deadline) => {
          match commands.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => {
              writer.tick(deadline);
              next_tick = Some(deadline + cadence);
              None
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
          }
        }
        None => match commands.recv() {
          Ok(command) => Some(command),
          Err(_) => return,
        },
      },
    };
    let Some(command) = command else { continue };
    match command {
      Command::Frame(mut frame) => {
        // Window and cropped-region capture only need the newest texture
        // before the next presentation tick. Discarding stale queued surfaces
        // prevents startup or crop/encoder pressure from becoming a permanent
        // multi-frame cursor delay. Full-screen and camera capture retain
        // every frame and keep their existing source-timestamp path.
        if writer.config.wall_timestamped_frames {
          loop {
            match commands.try_recv() {
              Ok(Command::Frame(newer)) => frame = newer,
              Ok(command) => {
                pending = Some(command);
                break;
              }
              Err(mpsc::TryRecvError::Empty) => break,
              Err(mpsc::TryRecvError::Disconnected) => return,
            }
          }
        }
        if writer.frame(frame) && !announced {
          announced = true;
          let _ = first_frame.send(Ok(()));
          if writer.config.wall_timestamped_frames {
            next_tick = Some(Instant::now() + cadence);
          }
        } else if !announced {
          if let Some(error) = writer.failed.clone() {
            let _ = first_frame.send(Err(error));
            return;
          }
        }
      }
      Command::Pause(at) => {
        let elapsed = writer.elapsed_ns(at);
        writer.timeline.pause(elapsed);
        if writer.config.wall_timestamped_frames {
          next_tick = None;
        }
      }
      Command::Resume(at) => {
        let elapsed = writer.elapsed_ns(at);
        writer.timeline.resume(elapsed);
        if writer.config.wall_timestamped_frames && writer.timeline.has_started() {
          next_tick = Some(at + cadence);
        }
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

  #[test]
  fn repeat_cadence_matches_selected_frame_rate() {
    assert_eq!(frame_cadence(30), Duration::from_nanos(33_333_333));
    assert_eq!(frame_cadence(60), Duration::from_nanos(16_666_666));
  }
}
