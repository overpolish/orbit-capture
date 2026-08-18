// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cursor timeline evaluation for the macOS GPU export.
//!
//! The export used to CPU-rasterise a motion-blurred cursor movie plus a
//! positions sidecar before Metal composited it. Only the timeline needs a
//! CPU: one small [`GpuCursor`] per output frame is all the shader needs to
//! scale, rotate, blur and blend the cursor itself.

use super::*;
use crate::exports::cursor_effects::{CursorCompositor, GpuArtwork, GpuCursor};

/// The cursor grid the compositor indexes. Frame `n` covers the output frames
/// whose presentation time is at or after `n / 60` seconds, which is exactly
/// what the retired positions sidecar encoded.
pub(super) const CURSOR_FRAME_RATE: u64 = 60;

pub(super) struct CursorTimeline {
  pub artworks: Vec<GpuArtwork>,
  pub frames: Vec<Option<GpuCursor>>,
}

pub(super) fn scaled_size(request: &CursorExportRequest<'_>) -> Result<(u32, u32), String> {
  let placement =
    crate::screenshots::output_placement(request.width, request.height, request.output)?;
  Ok((placement.image_width, placement.image_height))
}

/// Evaluates one cursor per 60 Hz timeline frame in output pixels. The cursor
/// is evaluated in the placed image's pixel space (so artwork size, hotspot
/// and motion-blur delta scale exactly as the retired pre-pass scaled them)
/// and then offset onto the canvas by the image origin.
pub(super) fn evaluate(
  request: &CursorExportRequest<'_>,
) -> Result<Option<CursorTimeline>, String> {
  let Some(cursor_path) = request.cursor else {
    return Ok(None);
  };
  let cursor = CursorCompositor::open(cursor_path)?;
  let (output_width, output_height) = scaled_size(request)?;
  let placement =
    crate::screenshots::output_placement(request.width, request.height, request.output)?;
  let frame_count = request
    .duration_ms
    .saturating_mul(CURSOR_FRAME_RATE)
    .div_ceil(1_000)
    .saturating_add(1);
  let frames = (0..frame_count)
    .map(|frame| {
      let position_ms = frame.saturating_mul(1_000) / CURSOR_FRAME_RATE;
      cursor
        .gpu_cursor(
          position_ms,
          (output_width, output_height),
          request.cursor_effects,
        )
        .map(|mut cursor| {
          cursor.x += placement.image_x as f32;
          cursor.y += placement.image_y as f32;
          cursor
        })
    })
    .collect();
  Ok(Some(CursorTimeline {
    artworks: crate::exports::cursor_effects::gpu_artworks(),
    frames,
  }))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    exports::cursor_effects::CursorEffectSettings,
    recording::cursor::{
      ButtonState, CursorButton, CursorRecord, CursorSource, CursorSourceKind, CursorStyle,
      FORMAT_VERSION,
    },
  };

  fn header() -> CursorRecord {
    CursorRecord::Header {
      coordinate_space: "global-logical-points".to_owned(),
      platform: "macos".to_owned(),
      source: CursorSource {
        height: 180.0,
        kind: CursorSourceKind::Screen,
        platform_id: "test".to_owned(),
        video_height: 180,
        video_width: 320,
        width: 320.0,
        x: 0.0,
        y: 0.0,
      },
      timebase: "recording-microseconds".to_owned(),
      version: FORMAT_VERSION,
    }
  }

  fn appearance(timestamp_us: u64) -> CursorRecord {
    CursorRecord::Appearance {
      height: 24.0,
      hotspot_x: 1.0,
      hotspot_y: 1.0,
      style: CursorStyle::Arrow,
      timestamp_us,
      width: 16.0,
    }
  }

  fn compositor(records: &[CursorRecord]) -> CursorCompositor {
    let directory = std::env::temp_dir().join(format!(
      "screenwide-gpu-cursor-{}-{:?}",
      std::process::id(),
      std::thread::current().id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("source.cursor.jsonl");
    let json = records
      .iter()
      .map(|record| serde_json::to_string(record).unwrap())
      .collect::<Vec<_>>()
      .join("\n");
    std::fs::write(&path, format!("{json}\n")).unwrap();
    let cursor = CursorCompositor::open(&path).unwrap();
    let _ = std::fs::remove_dir_all(&directory);
    cursor
  }

  #[test]
  fn evaluated_cursor_carries_the_frame_travel_as_its_blur_delta() {
    // Positions must stay inside one motion segment, so the sampled travel
    // has to be denser than POSITION_SEGMENT_GAP_US.
    let mut records = vec![header(), appearance(0)];
    records.extend((0..=50).map(|step| CursorRecord::Position {
      timestamp_us: step * 20_000,
      x: 80.0 + f64::from(step as u32) * 160.0 / 50.0,
      y: 80.0,
    }));
    let cursor = compositor(&records);
    let settings = CursorEffectSettings::default();
    let moving = cursor.gpu_cursor(500, (320, 180), settings).unwrap();

    assert!(
      moving.blur_delta_x > 0.0,
      "a cursor crossing the screen must blur along its travel"
    );
    assert!(moving.blur_delta_y.abs() < 0.001);
    // 160 output pixels per second at 60 Hz is a fraction under three pixels
    // of travel per frame.
    assert!(
      (moving.blur_delta_x - 160.0 / 60.0).abs() < 0.5,
      "unexpected frame travel {}",
      moving.blur_delta_x
    );

    let still = CursorEffectSettings {
      motion_blur: false,
      ..settings
    };
    let unblurred = cursor.gpu_cursor(500, (320, 180), still).unwrap();
    assert_eq!(unblurred.blur_delta_x, 0.0);
    assert_eq!(unblurred.blur_delta_y, 0.0);
  }

  #[test]
  fn evaluated_cursor_shrinks_while_a_button_is_held() {
    let cursor = compositor(&[
      header(),
      appearance(0),
      CursorRecord::Position {
        timestamp_us: 0,
        x: 80.0,
        y: 80.0,
      },
      CursorRecord::Button {
        button: CursorButton::Left,
        click_count: 1,
        state: ButtonState::Down,
        timestamp_us: 500_000,
        x: 80.0,
        y: 80.0,
      },
      CursorRecord::Button {
        button: CursorButton::Left,
        click_count: 1,
        state: ButtonState::Up,
        timestamp_us: 700_000,
        x: 80.0,
        y: 80.0,
      },
      CursorRecord::Position {
        timestamp_us: 2_000_000,
        x: 80.0,
        y: 80.0,
      },
    ]);
    let settings = CursorEffectSettings::default();
    let resting = cursor.gpu_cursor(100, (320, 180), settings).unwrap();
    let clicked = cursor.gpu_cursor(600, (320, 180), settings).unwrap();

    assert!(
      clicked.scale < resting.scale,
      "the click animation must scale the cursor down ({} vs {})",
      clicked.scale,
      resting.scale
    );

    let without_animation = CursorEffectSettings {
      click_animation: false,
      ..settings
    };
    let unanimated = cursor
      .gpu_cursor(600, (320, 180), without_animation)
      .unwrap();
    assert_eq!(unanimated.scale, resting.scale);
  }

  #[test]
  fn every_style_resolves_to_its_own_uploaded_artwork() {
    let artworks = crate::exports::cursor_effects::gpu_artworks();
    let cursor = compositor(&[
      header(),
      CursorRecord::Appearance {
        height: 24.0,
        hotspot_x: 1.0,
        hotspot_y: 1.0,
        style: CursorStyle::IBeam,
        timestamp_us: 0,
        width: 16.0,
      },
      CursorRecord::Position {
        timestamp_us: 0,
        x: 80.0,
        y: 80.0,
      },
      CursorRecord::Position {
        timestamp_us: 1_000_000,
        x: 80.0,
        y: 80.0,
      },
    ]);
    let evaluated = cursor
      .gpu_cursor(100, (320, 180), CursorEffectSettings::default())
      .unwrap();

    let artwork = artworks
      .get(evaluated.style as usize)
      .expect("the evaluated style indexes an uploaded artwork");
    assert!(artwork.width > 0 && artwork.height > 0);
    assert_eq!(
      artwork.pixels.len(),
      artwork.width as usize * artwork.height as usize * 4
    );
  }
}
