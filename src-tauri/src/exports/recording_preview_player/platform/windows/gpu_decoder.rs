// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Zero-system-memory Media Foundation reader for the live Windows preview.

use std::{path::Path, sync::Arc};

use windows::{
  core::{Interface, GUID, PCWSTR},
  Win32::{
    Graphics::Direct3D11::{ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE},
    Media::MediaFoundation::*,
    System::Com::StructuredStorage::PROPVARIANT,
  },
};

use super::decoder::MediaFoundation;
use crate::exports::preview_platform::RecordingPreviewSurface;

const HUNDRED_NS_PER_MS: i64 = 10_000;
// Media Foundation may position a fragmented MP4 at the first keyframe after
// an exact request. Near EOF that can mean no sample at all, leaving the
// compositor's previous frame visible. Rewind beyond the encoder's observed
// one-second GOP and decode forward to the requested presentation timestamp,
// matching macOS's preroll strategy while keeping every frame on the GPU.
const SETTLED_SEEK_PREROLL_MS: u64 = 1_500;
// Mid-drag seeks only have to look right, not be authoritative, and every
// prerolled millisecond is decode work standing between the pointer and the
// picture. Media Foundation already positions at the keyframe at or before
// the request, so decoding forward from there is the minimum possible work;
// any preroll only drags in the previous GOP as well. In the fragmented-MP4
// case where it lands on the keyframe after the request, a rough step simply
// shows a frame slightly ahead until the settled seek corrects it.
const ROUGH_SEEK_PREROLL_MS: u64 = 0;
const VIDEO_STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

fn win<T>(result: windows::core::Result<T>) -> Result<T, String> {
  result.map_err(|error| error.to_string())
}

#[derive(Clone)]
pub(crate) struct GpuFrame {
  pub(crate) height: u32,
  pub(crate) subresource: u32,
  pub(crate) texture: ID3D11Texture2D,
  pub(crate) timestamp_100ns: i64,
  pub(crate) timestamp_ms: u64,
  pub(crate) width: u32,
}

pub(crate) struct GpuVideoReader {
  height: u32,
  last_frame: Option<GpuFrame>,
  // Media Foundation pools the underlying DXGI surfaces through its samples.
  // Keep the current sample alive until the caller has presented the frame.
  last_sample: Option<IMFSample>,
  last_timestamp_ms: u64,
  reader: IMFSourceReader,
  width: u32,
  _device_manager: IMFDXGIDeviceManager,
  _runtime: MediaFoundation,
}

impl GpuVideoReader {
  pub(crate) fn open(
    path: &Path,
    start_ms: u64,
    surface: Arc<RecordingPreviewSurface>,
  ) -> Result<Self, String> {
    Self::open_with_device(path, start_ms, surface.device())
  }

  fn open_with_device(
    path: &Path,
    start_ms: u64,
    device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
  ) -> Result<Self, String> {
    let runtime = MediaFoundation::start()?;
    let mut reset_token = 0;
    let mut device_manager = None;
    win(unsafe { MFCreateDXGIDeviceManager(&mut reset_token, &mut device_manager) })?;
    let device_manager =
      device_manager.ok_or_else(|| "Media Foundation returned no DXGI manager".to_owned())?;
    win(unsafe { device_manager.ResetDevice(&device, reset_token) })?;

    let attributes = attributes(4)?;
    win(unsafe { attributes.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &device_manager) })?;
    win(unsafe { attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1) })?;
    win(unsafe { attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1) })?;
    let path = path
      .to_str()
      .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
    let wide = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let reader = win(unsafe { MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attributes) })?;
    win(unsafe { reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false) })?;
    win(unsafe { reader.SetStreamSelection(VIDEO_STREAM, true) })?;
    let native = win(unsafe { reader.GetNativeMediaType(VIDEO_STREAM, 0) })?;
    let native_size = win(unsafe { native.GetUINT64(&MF_MT_FRAME_SIZE) })?;
    let native_width = (native_size >> 32) as u32;
    let native_height = native_size as u32;
    let width = native_width.max(2) & !1;
    let height = native_height.max(2) & !1;

    let output = unsafe { MFCreateMediaType() }.map_err(|error| error.to_string())?;
    win(unsafe { output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) })?;
    win(unsafe { output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32) })?;
    win(unsafe {
      output.SetUINT64(
        &MF_MT_FRAME_SIZE,
        (u64::from(width) << 32) | u64::from(height),
      )
    })?;
    win(unsafe {
      output.SetUINT32(
        &MF_SA_D3D11_BINDFLAGS,
        (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
      )
    })?;
    win(unsafe { reader.SetCurrentMediaType(VIDEO_STREAM, None, &output) })?;
    let negotiated = win(unsafe { reader.GetCurrentMediaType(VIDEO_STREAM) })?;
    let packed = win(unsafe { negotiated.GetUINT64(&MF_MT_FRAME_SIZE) })?;
    let width = (packed >> 32) as u32;
    let height = packed as u32;
    if width == 0 || height == 0 {
      return Err("Media Foundation negotiated an empty GPU preview frame".to_owned());
    }
    let mut value = Self {
      height,
      last_frame: None,
      last_sample: None,
      last_timestamp_ms: 0,
      reader,
      width,
      _device_manager: device_manager,
      _runtime: runtime,
    };
    value.seek(start_ms, false)?;
    Ok(value)
  }

  #[cfg(test)]
  pub(super) const fn dimensions(&self) -> (u32, u32) {
    (self.width, self.height)
  }

  pub(super) fn seek(&mut self, position_ms: u64, rough: bool) -> Result<(), String> {
    win(unsafe { self.reader.Flush(VIDEO_STREAM) })?;
    let seek_ms = position_ms.saturating_sub(seek_preroll_ms(rough));
    let position = PROPVARIANT::from(
      i64::try_from(seek_ms)
        .unwrap_or(i64::MAX / HUNDRED_NS_PER_MS)
        .saturating_mul(HUNDRED_NS_PER_MS),
    );
    win(unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &position) })?;
    self.last_frame = None;
    self.last_sample = None;
    self.last_timestamp_ms = seek_ms;
    Ok(())
  }

  pub(crate) fn frame_at(&mut self, target_ms: u64) -> Result<Option<GpuFrame>, String> {
    loop {
      let Some(frame) = self.next_frame()? else {
        return Ok(self.last_frame.clone());
      };
      if frame.timestamp_ms.saturating_add(2) >= target_ms {
        return Ok(Some(frame));
      }
    }
  }

  /// Reads one real source sample without resampling it onto the preview's
  /// fixed display clock. Final export uses this so capture timestamps and
  /// dropped-frame gaps survive exactly instead of being shifted or padded.
  pub(crate) fn next_frame(&mut self) -> Result<Option<GpuFrame>, String> {
    loop {
      let mut flags = 0_u32;
      let mut timestamp = 0_i64;
      let mut sample = None;
      win(unsafe {
        self.reader.ReadSample(
          VIDEO_STREAM,
          0,
          None,
          Some(&mut flags),
          Some(&mut timestamp),
          Some(&mut sample),
        )
      })?;
      if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
        return Ok(None);
      }
      let Some(sample) = sample else {
        continue;
      };
      let timestamp_ms = u64::try_from(timestamp.max(0) / HUNDRED_NS_PER_MS).unwrap_or_default();
      let mut frame = sample_texture(&sample, self.width, self.height)?;
      frame.timestamp_100ns = timestamp.max(0);
      frame.timestamp_ms = timestamp_ms;
      self.last_sample = Some(sample);
      self.last_timestamp_ms = timestamp_ms;
      self.last_frame = Some(frame.clone());
      return Ok(Some(frame));
    }
  }

  pub(super) const fn last_timestamp_ms(&self) -> u64 {
    self.last_timestamp_ms
  }
}

/// Milliseconds of rewind a seek pays for before decoding forward. Settled
/// seeks buy accuracy near EOF; rough ones buy responsiveness.
pub(super) const fn seek_preroll_ms(rough: bool) -> u64 {
  if rough {
    ROUGH_SEEK_PREROLL_MS
  } else {
    SETTLED_SEEK_PREROLL_MS
  }
}

fn attributes(capacity: u32) -> Result<IMFAttributes, String> {
  let mut value = None;
  unsafe { MFCreateAttributes(&mut value, capacity) }.map_err(|error| error.to_string())?;
  value.ok_or_else(|| "Media Foundation created no GPU preview attributes".to_owned())
}

fn sample_texture(sample: &IMFSample, _width: u32, _height: u32) -> Result<GpuFrame, String> {
  let buffer = win(unsafe { sample.GetBufferByIndex(0) })?;
  let dxgi = buffer.cast::<IMFDXGIBuffer>().map_err(|_| {
    "Media Foundation returned a system-memory frame; GPU preview requires a DXGI surface"
      .to_owned()
  })?;
  let mut resource = std::ptr::null_mut();
  win(unsafe { dxgi.GetResource(&ID3D11Texture2D::IID, &mut resource) })?;
  let texture = unsafe { ID3D11Texture2D::from_raw(resource) };
  let subresource = win(unsafe { dxgi.GetSubresourceIndex() })?;
  let mut description = windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC::default();
  unsafe { texture.GetDesc(&mut description) };
  if description.Width == 0 || description.Height == 0 {
    return Err("Media Foundation returned an empty DXGI preview texture".to_owned());
  }
  Ok(GpuFrame {
    height: description.Height,
    subresource,
    texture,
    timestamp_100ns: 0,
    timestamp_ms: 0,
    width: description.Width,
  })
}

#[cfg(test)]
#[path = "gpu_decoder_tests.rs"]
mod tests;
