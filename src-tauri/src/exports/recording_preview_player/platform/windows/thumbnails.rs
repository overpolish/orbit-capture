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
  let Some(pane) = sources.playback_layout.panes.first() else {
    return;
  };
  let width = target_width(pane.source_width, pane.source_height);
  let mut reader = match NativeVideoReader::open(&sources.screen_path, width, THUMBNAIL_HEIGHT, 0) {
    Ok(reader) => reader,
    Err(_) => return,
  };
  for index in 0..count {
    let position_ms = timeline_position(sources.duration_ms, index, count);
    if reader.seek(position_ms).is_err() {
      continue;
    }
    let Ok(Some(frame)) = reader.frame_at(position_ms) else {
      continue;
    };
    let Ok(bytes) = encoded_jpeg(&frame, 80) else {
      continue;
    };
    if channel
      .send(InvokeResponseBody::Raw(payload(0, index, count, &bytes)))
      .is_err()
    {
      return;
    }
  }
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
  use super::timeline_position;

  #[test]
  fn final_thumbnail_stays_inside_the_last_timeline_bucket() {
    assert_eq!(timeline_position(9_000, 0, 9), 0);
    assert_eq!(timeline_position(9_000, 8, 9), 8_000);
  }
}
