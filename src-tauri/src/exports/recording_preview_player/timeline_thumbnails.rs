// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::thread;

use tauri::{image::Image, ipc::Channel, AppHandle};
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::{platform, sources};
use crate::exports::{
  cursor_effects::CursorEffectSettings, CameraOverlaySettings, RecordingOutputSettings,
};

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

/// Copies the composed primary frame at the playhead. Decoding and bitmap
/// composition happen only for this explicit action; live preview stays on
/// the native GPU surface and does not cross IPC.
#[tauri::command]
pub async fn copy_recording_preview_frame_to_clipboard(
  app: AppHandle,
  artifact_id: u64,
  position_ms: u64,
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  cursor_effects: CursorEffectSettings,
  recording_output: RecordingOutputSettings,
) -> Result<(), String> {
  let sources = sources(&app, artifact_id, None)?;
  let composed = tauri::async_runtime::spawn_blocking(move || {
    platform::composed_frame_image(
      &sources,
      position_ms,
      bake_camera,
      camera_overlay,
      cursor_effects,
      &recording_output,
    )
  })
  .await
  .map_err(|error| error.to_string())??;
  app
    .clipboard()
    .write_image(&Image::new(&composed.rgba, composed.width, composed.height))
    .map_err(|error| error.to_string())
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
