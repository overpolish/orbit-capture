// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Media metadata used by Windows export recovery without FFprobe.

use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

const HUNDRED_NS_PER_MS: u64 = 10_000;
const VIDEO_STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

#[derive(Clone, Copy, Debug)]
pub(in crate::exports) struct RecordingInfo {
  pub duration_ms: u64,
  pub height: u32,
  pub width: u32,
}

struct Runtime {
  uninitialize_com: bool,
}

impl Runtime {
  fn start() -> Option<Self> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let uninitialize_com = if initialized == RPC_E_CHANGED_MODE {
      false
    } else {
      initialized.ok().ok()?;
      true
    };
    if unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.is_err() {
      if uninitialize_com {
        unsafe { CoUninitialize() };
      }
      return None;
    }
    Some(Self { uninitialize_com })
  }
}

impl Drop for Runtime {
  fn drop(&mut self) {
    let _ = unsafe { MFShutdown() };
    if self.uninitialize_com {
      unsafe { CoUninitialize() };
    }
  }
}

pub(in crate::exports) fn recording_info(path: &Path) -> Option<RecordingInfo> {
  let path = path.to_path_buf();
  std::thread::Builder::new()
    .name("screenwide-recording-metadata-windows".to_owned())
    .spawn(move || recording_info_result(&path))
    .ok()?
    .join()
    .ok()?
    .ok()
}

fn recording_info_result(path: &Path) -> Result<RecordingInfo, String> {
  let _runtime = Runtime::start().ok_or_else(|| "Media Foundation could not start".to_owned())?;
  let path = path
    .to_str()
    .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
  let wide = path.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
  let reader = unsafe { MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), None) }
    .map_err(|error| error.to_string())?;
  let duration = unsafe {
    reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
  }
  .map_err(|error| error.to_string())?;
  let duration_100ns = u64::try_from(&duration).map_err(|error| error.to_string())?;
  let packed_size = unsafe { reader.GetNativeMediaType(VIDEO_STREAM, 0) }
    .ok()
    .and_then(|media_type| unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE) }.ok())
    .unwrap_or(0);
  let width = (packed_size >> 32) as u32;
  let height = packed_size as u32;
  if duration_100ns == 0 {
    return Err("Media Foundation returned empty recording metadata".to_owned());
  }
  Ok(RecordingInfo {
    duration_ms: duration_100ns.div_ceil(HUNDRED_NS_PER_MS),
    height,
    width,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use windows::Win32::System::Com::COINIT_APARTMENTTHREADED;

  #[test]
  #[ignore = "uses the video path in SCREENWIDE_WINDOWS_PREVIEW_TEST"]
  fn reads_recording_metadata_without_ffprobe() {
    let path = std::env::var_os("SCREENWIDE_WINDOWS_PREVIEW_TEST")
      .map(std::path::PathBuf::from)
      .expect("set SCREENWIDE_WINDOWS_PREVIEW_TEST to a recording");
    let info = recording_info_result(&path).unwrap();
    assert!(info.duration_ms > 1_000);
    assert!(info.width > 0 && info.height > 0);
  }

  #[test]
  #[ignore = "uses the video path in SCREENWIDE_WINDOWS_PREVIEW_TEST"]
  fn reads_recording_metadata_from_an_sta_caller() {
    let path = std::env::var_os("SCREENWIDE_WINDOWS_PREVIEW_TEST")
      .map(std::path::PathBuf::from)
      .expect("set SCREENWIDE_WINDOWS_PREVIEW_TEST to a recording");
    let info = std::thread::spawn(move || {
      unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .unwrap();
      let info = recording_info(&path);
      unsafe { CoUninitialize() };
      info
    })
    .join()
    .unwrap()
    .unwrap();
    assert!(info.duration_ms > 1_000);
    assert!(info.width > 0 && info.height > 0);
  }

  #[test]
  #[ignore = "uses the audio path in SCREENWIDE_WINDOWS_AUDIO_TEST"]
  fn reads_audio_only_metadata_without_ffprobe() {
    let path = std::env::var_os("SCREENWIDE_WINDOWS_AUDIO_TEST")
      .map(std::path::PathBuf::from)
      .expect("set SCREENWIDE_WINDOWS_AUDIO_TEST to an audio recording");
    let info = recording_info(&path).unwrap();
    assert!(info.duration_ms > 100);
    assert_eq!((info.width, info.height), (0, 0));
  }
}
