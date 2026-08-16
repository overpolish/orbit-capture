// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Media metadata used by macOS export recovery without FFprobe.

use std::path::Path;

use cidre::{av, ns};

use super::RecordingInfo;

pub(in crate::exports) fn recording_info(path: &Path) -> Option<RecordingInfo> {
  let path = path.to_path_buf();
  std::thread::Builder::new()
    .name("screenwide-recording-metadata-macos".to_owned())
    .spawn(move || recording_info_result(&path))
    .ok()?
    .join()
    .ok()?
    .ok()
}

fn recording_info_result(path: &Path) -> Result<RecordingInfo, String> {
  let path = path
    .to_str()
    .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
  let url = ns::Url::with_fs_path_str(path, false);
  let asset = av::UrlAsset::with_url(&url, None)
    .ok_or_else(|| "AVFoundation could not open the recovered recording".to_owned())?;
  let seconds = asset.duration().as_secs();
  if !seconds.is_finite() || seconds <= 0.0 {
    return Err("AVFoundation returned empty recording metadata".to_owned());
  }

  let tracks =
    tauri::async_runtime::block_on(asset.load_tracks_with_media_type(av::MediaType::video()))
      .map_err(|error| error.to_string())?;
  let (width, height) = tracks.get(0).map_or((0, 0), |track| {
    let size = track.natural_size();
    (
      size.width.abs().round() as u32,
      size.height.abs().round() as u32,
    )
  });

  Ok(RecordingInfo {
    duration_ms: (seconds * 1_000.0).round() as u64,
    height,
    width,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "uses the video path in SCREENWIDE_MACOS_PREVIEW_TEST"]
  fn reads_recording_metadata_with_avfoundation() {
    let path = std::env::var_os("SCREENWIDE_MACOS_PREVIEW_TEST")
      .map(std::path::PathBuf::from)
      .expect("set SCREENWIDE_MACOS_PREVIEW_TEST to a recording");
    let info = recording_info_result(&path).unwrap();
    assert!(info.duration_ms > 1_000);
    assert!(info.width > 0 && info.height > 0);
  }
}
