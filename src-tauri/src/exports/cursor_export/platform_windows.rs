// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows final-video composition. Media Foundation decodes into the same
//! D3D11 device used by the live preview, the preview shader draws offscreen,
//! and the hardware H.264 transform consumes that texture directly.

use std::{
  ffi::OsString,
  path::{Path, PathBuf},
  sync::atomic::{AtomicU64, Ordering},
};

use windows::{
  core::{Interface, PCWSTR},
  Win32::{
    Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D},
    Media::MediaFoundation::*,
  },
};

use super::*;
use crate::exports::{
  cursor_effects::CursorCompositor,
  preview_platform::{ComposedFrame, RecordingPreviewSurface},
  recording_preview_player::GpuVideoReader,
};

const EXPORT_FPS: u32 = 60;
const GPU_PROGRESS_PERCENT: u64 = 95;
static EXPORT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

fn win<T>(result: windows::core::Result<T>) -> Result<T, String> {
  result.map_err(|error| error.to_string())
}

struct VideoSink {
  _byte_stream: IMFByteStream,
  _device_manager: IMFDXGIDeviceManager,
  media_sink: IMFMediaSink,
  sink: Option<IMFSinkWriter>,
}

impl VideoSink {
  fn new(
    path: &Path,
    device: &ID3D11Device,
    width: u32,
    height: u32,
    bitrate: u64,
  ) -> Result<Self, String> {
    let mut reset_token = 0;
    let mut manager = None;
    win(unsafe { MFCreateDXGIDeviceManager(&mut reset_token, &mut manager) })?;
    let manager =
      manager.ok_or_else(|| "Media Foundation created no export D3D manager".to_owned())?;
    win(unsafe { manager.ResetDevice(device, reset_token) })?;

    let attributes = attributes(4)?;
    win(unsafe { attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1) })?;
    win(unsafe { attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager) })?;
    let output = video_type(MFVideoFormat_H264, width, height)?;
    win(unsafe { output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate.min(u64::from(u32::MAX)) as u32) })?;
    win(unsafe { output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32) })?;
    win(unsafe { output.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, EXPORT_FPS) })?;

    let wide = path
      .to_str()
      .ok_or_else(|| "The export path is not valid UTF-8".to_owned())?
      .encode_utf16()
      .chain(Some(0))
      .collect::<Vec<_>>();
    let byte_stream = win(unsafe {
      MFCreateFile(
        MF_ACCESSMODE_WRITE,
        MF_OPENMODE_DELETE_IF_EXIST,
        MF_FILEFLAGS_NONE,
        PCWSTR(wide.as_ptr()),
      )
    })?;
    let media_sink = win(unsafe { MFCreateFMPEG4MediaSink(&byte_stream, &output, None) })?;
    let sink = win(unsafe { MFCreateSinkWriterFromMediaSink(&media_sink, &attributes) })?;
    let input = video_type(MFVideoFormat_ARGB32, width, height)?;
    win(unsafe { sink.SetInputMediaType(0, &input, None) })?;
    win(unsafe { sink.BeginWriting() })?;
    Ok(Self {
      _byte_stream: byte_stream,
      _device_manager: manager,
      media_sink,
      sink: Some(sink),
    })
  }

  fn write(
    &self,
    texture: &ID3D11Texture2D,
    pts_100ns: i64,
    duration_100ns: i64,
  ) -> Result<(), String> {
    let buffer = unsafe { MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false) }
      .map_err(|error| error.to_string())?;
    let two_dimensional = buffer
      .cast::<IMF2DBuffer>()
      .map_err(|error| error.to_string())?;
    let length =
      unsafe { two_dimensional.GetContiguousLength() }.map_err(|error| error.to_string())?;
    win(unsafe { buffer.SetCurrentLength(length) })?;
    let sample = unsafe { MFCreateSample() }.map_err(|error| error.to_string())?;
    win(unsafe { sample.AddBuffer(&buffer) })?;
    win(unsafe { sample.SetSampleTime(pts_100ns.max(0)) })?;
    win(unsafe { sample.SetSampleDuration(duration_100ns.max(1)) })?;
    let sink = self
      .sink
      .as_ref()
      .ok_or_else(|| "The export encoder is already finalized".to_owned())?;
    win(unsafe { sink.WriteSample(0, &sample) })
  }

  fn finish(mut self) -> Result<(), String> {
    let sink = self
      .sink
      .take()
      .ok_or_else(|| "The export encoder is already finalized".to_owned())?;
    win(unsafe { sink.Finalize() })?;
    drop(sink);
    win(unsafe { self.media_sink.Shutdown() })
  }
}

impl Drop for VideoSink {
  fn drop(&mut self) {
    self.sink.take();
    let _ = unsafe { self.media_sink.Shutdown() };
  }
}

fn attributes(capacity: u32) -> Result<IMFAttributes, String> {
  let mut value = None;
  win(unsafe { MFCreateAttributes(&mut value, capacity) })?;
  value.ok_or_else(|| "Media Foundation created no export attributes".to_owned())
}

fn video_type(
  subtype: windows::core::GUID,
  width: u32,
  height: u32,
) -> Result<IMFMediaType, String> {
  let value = unsafe { MFCreateMediaType() }.map_err(|error| error.to_string())?;
  win(unsafe { value.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })?;
  win(unsafe { value.SetGUID(&MF_MT_SUBTYPE, &subtype) })?;
  win(unsafe {
    value.SetUINT64(
      &MF_MT_FRAME_SIZE,
      (u64::from(width) << 32) | u64::from(height),
    )
  })?;
  win(unsafe { value.SetUINT64(&MF_MT_FRAME_RATE, (u64::from(EXPORT_FPS) << 32) | 1) })?;
  win(unsafe { value.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1_u64 << 32) | 1) })?;
  win(unsafe { value.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32) })?;
  Ok(value)
}

fn gpu_video_path() -> PathBuf {
  let attempt = EXPORT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
  std::env::temp_dir().join(format!(
    "{}windows-gpu-video-{}-{attempt}.mp4",
    media_preview::PREVIEW_PREFIX,
    std::process::id()
  ))
}

fn render_video(
  request: &mut CursorExportRequest<'_>,
  path: &Path,
) -> Result<ExportRunResult, String> {
  crate::screenshots::validate_output_settings(request.width, request.height, request.output)?;
  let surface = RecordingPreviewSurface::existing()?;
  let mut reader = GpuVideoReader::open(request.screen, 0, surface.clone())?;
  let mut camera_reader = request
    .camera
    .map(|(path, _)| GpuVideoReader::open(path, 0, surface.clone()))
    .transpose()?;
  let mut camera_current = None;
  let mut camera_next = match camera_reader.as_mut() {
    Some(reader) => reader.next_frame()?,
    None => None,
  };
  let mut cursor = request.cursor.map(CursorCompositor::open).transpose()?;
  let output_size = crate::screenshots::output_dimensions(request.output)?;
  let compositor = request.camera.map_or_else(
    || surface.export_compositor((request.width, request.height), output_size),
    |(_, options)| {
      surface.export_compositor_with_camera(
        (request.width, request.height),
        (options.camera_width, options.camera_height),
        output_size,
      )
    },
  )?;
  let camera_geometry = request
    .camera
    .map(|(_, options)| media_preview::bake_geometry(options))
    .transpose()?;
  let sink = VideoSink::new(
    path,
    &surface.device(),
    output_size.0,
    output_size.1,
    super::video_bitrate(output_size.0, output_size.1, request.video.compression),
  )?;
  let mut current = reader
    .next_frame()?
    .ok_or_else(|| "Media Foundation returned no frame for export".to_owned())?;
  let timeline_origin = current.timestamp_100ns;
  let export_end_100ns = i64::try_from(request.duration_ms)
    .unwrap_or(i64::MAX / 10_000)
    .saturating_mul(10_000);
  loop {
    if request.cancelled.load(Ordering::Acquire) {
      let _ = std::fs::remove_file(path);
      return Ok(ExportRunResult::Cancelled);
    }
    let next = reader.next_frame()?;
    let pts_100ns = current.timestamp_100ns.saturating_sub(timeline_origin);
    let next_pts_100ns = next.as_ref().map_or(export_end_100ns, |frame| {
      frame.timestamp_100ns.saturating_sub(timeline_origin)
    });
    let position_ms = u64::try_from(pts_100ns.max(0) / 10_000).unwrap_or_default();
    while camera_next
      .as_ref()
      .is_some_and(|frame| frame.timestamp_100ns.saturating_sub(timeline_origin) <= pts_100ns)
    {
      camera_current = camera_next.take();
      camera_next = match camera_reader.as_mut() {
        Some(reader) => reader.next_frame()?,
        None => None,
      };
    }
    let baked_cursor = cursor.as_mut().and_then(|cursor| {
      cursor.gpu_cursor(
        position_ms,
        (current.width, current.height),
        request.cursor_effects,
      )
    });
    let camera_frame = camera_current.as_ref().and_then(|frame| {
      request.camera.map(|(_, options)| {
        (
          &frame.texture,
          frame.subresource,
          camera_geometry.expect("camera geometry exists with camera options"),
          options.camera_drop_shadow,
          request.camera_on_top,
        )
      })
    });
    let texture = compositor.compose_with_camera(
      &current.texture,
      current.subresource,
      request.output,
      ComposedFrame {
        cursor: baked_cursor,
        foreground_only: false,
        seconds: position_ms as f64 / 1_000.0,
      },
      camera_frame,
    )?;
    sink.write(
      &texture,
      pts_100ns,
      next_pts_100ns.saturating_sub(pts_100ns),
    )?;
    (request.on_progress)(
      position_ms
        .min(request.duration_ms)
        .saturating_mul(GPU_PROGRESS_PERCENT)
        / 100,
    );
    let Some(next) = next else {
      break;
    };
    current = next;
  }
  sink.finish()?;
  Ok(ExportRunResult::Completed)
}

fn mux_args(request: &CursorExportRequest<'_>, video: &Path, temporary: &Path) -> Vec<OsString> {
  let mut args = ["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-i"]
    .map(OsString::from)
    .to_vec();
  args.push(video.into());
  args.extend([
    OsString::from("-i"),
    request.audio_source.unwrap_or(request.screen).into(),
  ]);
  args.extend(
    [
      "-progress",
      "pipe:1",
      "-nostats",
      "-map",
      "0:v:0",
      "-c:v",
      "copy",
    ]
    .map(OsString::from),
  );
  args.extend(
    request
      .selection
      .audio_args_from(request.audio_layout, 1)
      .into_iter()
      .map(OsString::from),
  );
  args.extend(
    [
      "-tag:v",
      "avc1",
      "-movflags",
      "+faststart",
      "-map_metadata",
      "-1",
      "-f",
      "mp4",
    ]
    .map(OsString::from),
  );
  args.push(temporary.into());
  args
}

pub(super) fn export(mut request: CursorExportRequest<'_>) -> Result<ExportRunResult, String> {
  let video = gpu_video_path();
  let result = (|| {
    let rendered = render_video(&mut request, &video)?;
    if !matches!(rendered, ExportRunResult::Completed) {
      return Ok(rendered);
    }
    let temporary = media_preview::remux_temp_path(request.destination);
    let args = mux_args(&request, &video, &temporary);
    let duration_ms = request.duration_ms;
    let on_progress = &mut request.on_progress;
    let mut final_progress = |processed_ms: u64| {
      on_progress(
        duration_ms.saturating_mul(GPU_PROGRESS_PERCENT) / 100
          + processed_ms.saturating_mul(100 - GPU_PROGRESS_PERCENT) / 100,
      );
    };
    media_preview::run_export(
      args,
      &temporary,
      request.destination,
      request.cancelled,
      &mut final_progress,
    )
  })();
  let _ = std::fs::remove_file(&video);
  result
}
