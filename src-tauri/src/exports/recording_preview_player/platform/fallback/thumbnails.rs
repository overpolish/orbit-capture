// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! FFmpeg timeline thumbnails for platforms without a native decoder.

use tauri::ipc::Channel;

use crate::exports::recording_preview_player::{
  timeline_thumbnails::{payload, target_width, THUMBNAIL_HEIGHT},
  PlayerSources,
};

fn generate_track(
  path: &std::path::Path,
  pane: &crate::exports::recording_preview_player::layout::PreviewPane,
  duration_ms: u64,
  count: u32,
  track: u32,
  channel: &Channel,
) -> bool {
  use std::process::Command;

  use tauri::ipc::InvokeResponseBody;

  let filter = format!(
    "fps={}/{}:round=up,scale={}:{}:flags=fast_bilinear",
    u64::from(count) * 1_000,
    duration_ms.max(1),
    target_width(pane.source_width, pane.source_height),
    THUMBNAIL_HEIGHT
  );
  let count_text = count.to_string();
  let Ok(output) = Command::new(crate::exports::media_preview::ffmpeg_path())
    .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
    .arg(path)
    .args([
      "-vf",
      &filter,
      "-frames:v",
      &count_text,
      "-an",
      "-c:v",
      "mjpeg",
      "-q:v",
      "5",
      "-f",
      "image2pipe",
      "pipe:1",
    ])
    .output()
  else {
    return false;
  };
  if !output.status.success() {
    return false;
  }
  let mut remaining = output.stdout.as_slice();
  for index in 0..count {
    let Some(start) = remaining.windows(2).position(|pair| pair == [0xff, 0xd8]) else {
      break;
    };
    remaining = &remaining[start..];
    let Some(end) = remaining
      .windows(2)
      .position(|pair| pair == [0xff, 0xd9])
      .map(|position| position + 2)
    else {
      break;
    };
    if channel
      .send(InvokeResponseBody::Raw(payload(
        track,
        index,
        count,
        &remaining[..end],
      )))
      .is_err()
    {
      return false;
    }
    remaining = &remaining[end..];
  }
  true
}

pub(super) fn generate(sources: PlayerSources, count: u32, channel: Channel) {
  let Some(primary_pane) = sources.playback_layout.panes.first() else {
    return;
  };
  if !generate_track(
    &sources.screen_path,
    primary_pane,
    sources.duration_ms,
    count,
    0,
    &channel,
  ) {
    return;
  }
  if let (Some(path), Some(pane), Some(duration_ms)) = (
    sources.camera_path.as_deref(),
    sources.playback_layout.panes.get(1),
    sources.camera_duration_ms,
  ) {
    generate_track(path, pane, duration_ms, count, 1, &channel);
  }
}

pub(super) fn source_frame_jpeg(
  path: &std::path::Path,
  position_ms: u64,
  duration_ms: u64,
) -> Result<Vec<u8>, String> {
  let _ = (path, position_ms, duration_ms);
  Err("Source frames are only available on macOS".to_owned())
}
