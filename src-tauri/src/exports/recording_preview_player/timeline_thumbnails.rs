// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::thread;

use tauri::{ipc::Channel, AppHandle};

use super::{platform, sources};

pub(super) const HEADER_MARKER: u32 = u32::from_le_bytes(*b"OCTH");
const HEADER_VERSION: u32 = 1;
const MAX_THUMBNAILS: u32 = 32;
const MIN_THUMBNAILS: u32 = 4;
pub(super) const THUMBNAIL_HEIGHT: u32 = 64;

pub(super) fn payload(track: u32, index: u32, count: u32, jpeg: &[u8]) -> Vec<u8> {
  let mut payload = Vec::with_capacity(20 + jpeg.len());
  payload.extend_from_slice(&HEADER_MARKER.to_le_bytes());
  payload.extend_from_slice(&HEADER_VERSION.to_le_bytes());
  payload.extend_from_slice(&track.to_le_bytes());
  payload.extend_from_slice(&index.to_le_bytes());
  payload.extend_from_slice(&count.to_le_bytes());
  payload.extend_from_slice(jpeg);
  payload
}

pub(super) fn target_width(source_width: u32, source_height: u32) -> u32 {
  if source_width == 0 || source_height == 0 {
    return THUMBNAIL_HEIGHT;
  }
  ((f64::from(source_width) * f64::from(THUMBNAIL_HEIGHT) / f64::from(source_height))
    .round()
    .max(1.0)) as u32
}

/// One full-resolution source frame as JPEG, so webview tools that need real
/// pixels (the crop magnifier) have a bitmap even though display happens on
/// the native surface below the webview.
#[tauri::command]
pub async fn copy_recording_preview_source_frame(
  app: AppHandle,
  artifact_id: u64,
  position_ms: u64,
  track: u32,
) -> Result<tauri::ipc::Response, String> {
  let sources = sources(&app, artifact_id, None)?;
  let (path, duration_ms) = if track == 1 {
    (
      sources
        .camera_path
        .clone()
        .ok_or_else(|| "The recording has no camera track".to_owned())?,
      sources.camera_duration_ms.unwrap_or(sources.duration_ms),
    )
  } else {
    (sources.screen_path.clone(), sources.duration_ms)
  };
  let jpeg = tauri::async_runtime::spawn_blocking(move || {
    platform::source_frame_jpeg(&path, position_ms, duration_ms)
  })
  .await
  .map_err(|error| error.to_string())??;
  Ok(tauri::ipc::Response::new(jpeg))
}

#[tauri::command]
pub async fn stream_recording_timeline_thumbnails(
  app: AppHandle,
  artifact_id: u64,
  count: u32,
  channel: Channel,
) -> Result<(), String> {
  let sources = sources(&app, artifact_id, None)?;
  let count = count.clamp(MIN_THUMBNAILS, MAX_THUMBNAILS);
  thread::Builder::new()
    .name("recording-timeline-thumbnails".to_owned())
    .spawn(move || platform::generate_thumbnails(sources, count, channel))
    .map_err(|error| error.to_string())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_thumbnail_width_in_source_aspect_ratio() {
    assert_eq!(target_width(1_920, 1_080), 114);
    assert_eq!(target_width(1_080, 1_920), 36);
  }

  #[test]
  fn payload_has_a_stable_header() {
    let bytes = payload(1, 2, 20, &[3, 4]);
    assert_eq!(&bytes[..4], b"OCTH");
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 1);
    assert_eq!(&bytes[20..], &[3, 4]);
  }
}
