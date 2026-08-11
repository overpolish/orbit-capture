// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::thread;

use tauri::{ipc::Channel, AppHandle};

use super::{sources, PlayerSources};

const HEADER_MARKER: u32 = u32::from_le_bytes(*b"OCTH");
const HEADER_VERSION: u32 = 1;
const MAX_THUMBNAILS: u32 = 32;
const MIN_THUMBNAILS: u32 = 4;
const THUMBNAIL_HEIGHT: u32 = 64;

fn payload(track: u32, index: u32, count: u32, jpeg: &[u8]) -> Vec<u8> {
  let mut payload = Vec::with_capacity(20 + jpeg.len());
  payload.extend_from_slice(&HEADER_MARKER.to_le_bytes());
  payload.extend_from_slice(&HEADER_VERSION.to_le_bytes());
  payload.extend_from_slice(&track.to_le_bytes());
  payload.extend_from_slice(&index.to_le_bytes());
  payload.extend_from_slice(&count.to_le_bytes());
  payload.extend_from_slice(jpeg);
  payload
}

fn target_width(source_width: u32, source_height: u32) -> u32 {
  if source_width == 0 || source_height == 0 {
    return THUMBNAIL_HEIGHT;
  }
  ((f64::from(source_width) * f64::from(THUMBNAIL_HEIGHT) / f64::from(source_height))
    .round()
    .max(1.0)) as u32
}

#[cfg(target_os = "macos")]
fn generate(sources: PlayerSources, count: u32, channel: Channel) {
  use cidre::cg;
  use tauri::ipc::InvokeResponseBody;

  use super::still_macos::{frame_position, image_generator, images_at};

  let Some(primary_pane) = sources.playback_layout.panes.first() else {
    return;
  };
  let mut primary = match image_generator(&sources.screen_path) {
    Ok(generator) => generator,
    Err(_) => return,
  };
  primary.set_max_size(cg::Size {
    width: f64::from(target_width(
      primary_pane.source_width,
      primary_pane.source_height,
    )),
    height: f64::from(THUMBNAIL_HEIGHT),
  });
  let mut camera = match sources.camera_path.as_deref().map(image_generator) {
    Some(Ok(generator)) => Some(generator),
    Some(Err(_)) | None => None,
  };
  if let (Some(generator), Some(pane)) = (camera.as_mut(), sources.playback_layout.panes.get(1)) {
    generator.set_max_size(cg::Size {
      width: f64::from(target_width(pane.source_width, pane.source_height)),
      height: f64::from(THUMBNAIL_HEIGHT),
    });
  }

  for index in 0..count {
    let denominator = u64::from(count.saturating_sub(1).max(1));
    let position_ms = sources
      .duration_ms
      .saturating_sub(1)
      .saturating_mul(u64::from(index))
      / denominator;
    let camera_position_ms = sources
      .camera_duration_ms
      .map(|duration| frame_position(position_ms, duration));
    let Ok((primary_jpeg, camera_jpeg)) = images_at(
      &primary,
      camera.as_deref(),
      frame_position(position_ms, sources.duration_ms),
      camera_position_ms,
    ) else {
      continue;
    };
    if channel
      .send(InvokeResponseBody::Raw(payload(
        0,
        index,
        count,
        &primary_jpeg,
      )))
      .is_err()
    {
      return;
    }
    if let Some(camera_jpeg) = camera_jpeg {
      if channel
        .send(InvokeResponseBody::Raw(payload(
          1,
          index,
          count,
          &camera_jpeg,
        )))
        .is_err()
      {
        return;
      }
    }
  }
}

#[cfg(not(target_os = "macos"))]
fn generate_track(
  path: &std::path::Path,
  pane: &super::layout::PreviewPane,
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

#[cfg(not(target_os = "macos"))]
fn generate(sources: PlayerSources, count: u32, channel: Channel) {
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

#[tauri::command]
pub async fn stream_recording_timeline_thumbnails(
  app: AppHandle,
  artifact_id: u64,
  count: u32,
  channel: Channel,
) -> Result<(), String> {
  let sources = sources(&app, artifact_id)?;
  let count = count.clamp(MIN_THUMBNAILS, MAX_THUMBNAILS);
  thread::Builder::new()
    .name("recording-timeline-thumbnails".to_owned())
    .spawn(move || generate(sources, count, channel))
    .map_err(|error| error.to_string())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(target_os = "macos")]
  #[test]
  #[ignore = "uses the video path in ORBIT_CAPTURE_THUMBNAIL_BENCHMARK"]
  fn benchmarks_thirty_minute_native_thumbnail_extraction() {
    use std::{path::Path, time::Instant};

    use cidre::cg;

    use crate::exports::recording_preview_player::still_macos::{image_generator, images_at};

    let path = std::env::var("ORBIT_CAPTURE_THUMBNAIL_BENCHMARK")
      .expect("set ORBIT_CAPTURE_THUMBNAIL_BENCHMARK to a 30-minute video");
    let mut primary = image_generator(Path::new(&path)).expect("the benchmark video should open");
    primary.set_max_size(cg::Size {
      width: 114.0,
      height: f64::from(THUMBNAIL_HEIGHT),
    });
    let mut camera =
      image_generator(Path::new(&path)).expect("the benchmark camera video should open");
    camera.set_max_size(cg::Size {
      width: 114.0,
      height: f64::from(THUMBNAIL_HEIGHT),
    });
    let started = Instant::now();
    let mut encoded_bytes = 0;
    for index in 0..24 {
      let position_ms = 1_800_000_u64.saturating_sub(1) * index / 23;
      let (primary_jpeg, camera_jpeg) =
        images_at(&primary, Some(&camera), position_ms, Some(position_ms))
          .expect("every benchmark thumbnail should decode");
      encoded_bytes += primary_jpeg.len();
      encoded_bytes += camera_jpeg
        .expect("the camera thumbnail should decode")
        .len();
    }
    let elapsed = started.elapsed();
    eprintln!(
      "decoded 48 thumbnails from two 30-minute tracks in {elapsed:?} ({} bytes)",
      encoded_bytes
    );
    assert!(encoded_bytes > 0);
  }

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
