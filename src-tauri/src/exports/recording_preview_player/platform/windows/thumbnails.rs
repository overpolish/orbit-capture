// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native Media Foundation timeline thumbnails and source-frame extraction.

use tauri::ipc::{Channel, InvokeResponseBody};

use super::decoder::{encoded_jpeg, NativeVideoReader};
use crate::exports::recording_preview_player::{
  timeline_thumbnails::{payload, target_width, THUMBNAIL_HEIGHT},
  PlayerSources,
};

pub(super) fn generate(sources: PlayerSources, count: u32, channel: Channel) {
  let mut tracks = vec![(0_u32, &sources.screen_path, sources.duration_ms)];
  if let Some(path) = &sources.camera_path {
    tracks.push((
      1,
      path,
      sources.camera_duration_ms.unwrap_or(sources.duration_ms),
    ));
  }
  for (track, path, duration_ms) in tracks {
    let Some(pane) = sources.playback_layout.panes.get(track as usize) else {
      continue;
    };
    let width = target_width(pane.source_width, pane.source_height);
    let mut reader = match NativeVideoReader::open(path, width, THUMBNAIL_HEIGHT, 0) {
      Ok(reader) => reader,
      Err(_) => continue,
    };
    let mut last_thumbnail = None;
    for index in 0..count {
      let position_ms = timeline_position(duration_ms, index, count);
      let decoded = reader
        .seek(position_ms)
        .and_then(|()| reader.frame_at(position_ms))
        .ok()
        .flatten()
        .and_then(|frame| encoded_jpeg(&frame, 80).ok());
      // Sidecars can end a fraction before the shared recording timeline.
      // Keep the strip complete with the most recent real camera frame rather
      // than exposing the frontend's grey missing-thumbnail placeholder.
      let Some(bytes) = thumbnail_or_previous(decoded, &last_thumbnail) else {
        continue;
      };
      last_thumbnail = Some(bytes.clone());
      if channel
        .send(InvokeResponseBody::Raw(payload(
          track, index, count, &bytes,
        )))
        .is_err()
      {
        return;
      }
    }
  }
}

fn thumbnail_or_previous(decoded: Option<Vec<u8>>, previous: &Option<Vec<u8>>) -> Option<Vec<u8>> {
  decoded.or_else(|| previous.clone())
}

// Sample the beginning of each timeline bucket. In particular, the final
// thumbnail stays inside the last bucket instead of asking Media Foundation
// for the container endpoint, where no decodable frame has to exist.
fn timeline_position(duration_ms: u64, index: u32, count: u32) -> u64 {
  duration_ms.saturating_mul(u64::from(index)) / u64::from(count.max(1))
}

pub(super) fn source_frame_jpeg(
  path: &std::path::Path,
  position_ms: u64,
  duration_ms: u64,
) -> Result<Vec<u8>, String> {
  let position_ms = position_ms.min(duration_ms.saturating_sub(1));
  let mut reader = NativeVideoReader::open(path, 0, 0, position_ms)?;
  let frame = reader
    .frame_at(position_ms)?
    .ok_or_else(|| "Media Foundation returned no source frame".to_owned())?;
  encoded_jpeg(&frame, 92)
}

#[cfg(test)]
mod tests {
  use super::{thumbnail_or_previous, timeline_position};

  #[test]
  fn final_thumbnail_stays_inside_the_last_timeline_bucket() {
    assert_eq!(timeline_position(9_000, 0, 9), 0);
    assert_eq!(timeline_position(9_000, 8, 9), 8_000);
  }

  #[test]
  fn a_short_sidecar_reuses_its_last_real_thumbnail() {
    let previous = Some(vec![1, 2, 3]);
    assert_eq!(thumbnail_or_previous(None, &previous), previous);
    assert_eq!(
      thumbnail_or_previous(Some(vec![4]), &previous),
      Some(vec![4])
    );
  }
}
