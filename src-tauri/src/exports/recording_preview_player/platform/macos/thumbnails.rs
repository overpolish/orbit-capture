// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS timeline thumbnails and full-resolution source frames.
//!
//! `AVAssetImageGenerator` decodes both tracks concurrently at the thumbnail
//! size, so a long recording's strip fills without ever holding a
//! full-resolution frame.

use tauri::ipc::Channel;

use super::image::frame_position;
use crate::exports::recording_preview_player::{
  timeline_thumbnails::{payload, target_width, THUMBNAIL_HEIGHT},
  PlayerSources,
};

fn image_generator(
  path: &std::path::Path,
) -> Result<cidre::arc::R<cidre::av::AssetImageGenerator>, String> {
  use cidre::{av, cm, ns};

  let path_text = path
    .to_str()
    .ok_or_else(|| "The recording path is not valid UTF-8".to_owned())?;
  let url = ns::Url::with_fs_path_str(path_text, false);
  let asset = av::UrlAsset::with_url(&url, None)
    .ok_or_else(|| format!("AVFoundation could not open {}", path.display()))?;
  let mut generator = av::AssetImageGenerator::with_asset(&asset);
  generator.set_applies_preferred_track_transform(true);
  generator.set_requested_time_tolerance_before(cm::Time::new(100, 1_000));
  generator.set_requested_time_tolerance_after(cm::Time::new(1, 60));
  Ok(generator)
}

fn images_at(
  screen: &cidre::av::AssetImageGenerator,
  camera: Option<&cidre::av::AssetImageGenerator>,
  screen_position_ms: u64,
  camera_position_ms: Option<u64>,
) -> Result<
  (
    crate::screenshots::CapturedImage,
    Option<crate::screenshots::CapturedImage>,
  ),
  String,
> {
  use super::image::{captured_image, time};

  let screen_time = time(screen_position_ms);
  if let (Some(camera), Some(camera_position_ms)) = (camera, camera_position_ms) {
    let camera_time = time(camera_position_ms);
    let (screen, camera) = tauri::async_runtime::block_on(async {
      tokio::join!(
        screen.cg_image_for_time(screen_time),
        camera.cg_image_for_time(camera_time)
      )
    });
    let (screen, _) = screen.map_err(|error| error.to_string())?;
    let (camera, _) = camera.map_err(|error| error.to_string())?;
    Ok((captured_image(&screen)?, Some(captured_image(&camera)?)))
  } else {
    let (screen, _) = tauri::async_runtime::block_on(screen.cg_image_for_time(screen_time))
      .map_err(|error| error.to_string())?;
    Ok((captured_image(&screen)?, None))
  }
}

pub(super) fn generate(sources: PlayerSources, count: u32, channel: Channel) {
  use cidre::cg;
  use tauri::ipc::InvokeResponseBody;

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
    let Ok((primary_image, camera_image)) = images_at(
      &primary,
      camera.as_deref(),
      frame_position(position_ms, sources.duration_ms),
      camera_position_ms,
    ) else {
      continue;
    };
    let Ok(primary_jpeg) = super::composition::encoded_jpeg(&primary_image) else {
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
    if let Some(camera_image) = camera_image {
      let Ok(camera_jpeg) = super::composition::encoded_jpeg(&camera_image) else {
        continue;
      };
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

pub(super) fn source_frame_jpeg(
  path: &std::path::Path,
  position_ms: u64,
  duration_ms: u64,
) -> Result<Vec<u8>, String> {
  let generator = image_generator(path)?;
  let position = frame_position(position_ms.min(duration_ms), duration_ms);
  let (image, _) =
    tauri::async_runtime::block_on(generator.cg_image_for_time(super::image::time(position)))
      .map_err(|error| error.to_string())?;
  let captured = super::image::captured_image(&image)?;
  super::composition::encoded_jpeg(&captured)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "uses the video path in SCREENWIDE_THUMBNAIL_BENCHMARK"]
  fn benchmarks_thirty_minute_native_thumbnail_extraction() {
    use std::{path::Path, time::Instant};

    use cidre::cg;

    let path = std::env::var("SCREENWIDE_THUMBNAIL_BENCHMARK")
      .expect("set SCREENWIDE_THUMBNAIL_BENCHMARK to a 30-minute video");
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
      let (primary_image, camera_image) =
        images_at(&primary, Some(&camera), position_ms, Some(position_ms))
          .expect("every benchmark thumbnail should decode");
      let primary_jpeg = super::super::composition::encoded_jpeg(&primary_image)
        .expect("the primary thumbnail should encode");
      let camera_jpeg = super::super::composition::encoded_jpeg(
        &camera_image.expect("the camera thumbnail should decode"),
      )
      .expect("the camera thumbnail should encode");
      encoded_bytes += primary_jpeg.len();
      encoded_bytes += camera_jpeg.len();
    }
    let elapsed = started.elapsed();
    eprintln!(
      "decoded 48 thumbnails from two 30-minute tracks in {elapsed:?} ({} bytes)",
      encoded_bytes
    );
    assert!(encoded_bytes > 0);
  }
}
