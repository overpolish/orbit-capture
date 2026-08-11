// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Final-video cursor composition.
//!
//! Rust renders only a small transparent cursor movie. On macOS, decoded screen
//! planes stay in Core Video, Metal copies them and blends only the cursor's
//! bounds, then VideoToolbox encodes the result. Selected audio is stream-copied
//! or mixed afterwards without decoding the finished video.

use std::{path::Path, sync::atomic::AtomicBool};

use super::{
  cursor_effects::CursorEffectSettings,
  media_preview::{self, BakedVideoExportOptions, ExportRunResult, VideoExportOptions},
  track_selection::{AudioLayout, TrackSelection},
};

#[cfg(target_os = "macos")]
#[path = "cursor_export/native_macos.rs"]
mod native_macos;
#[cfg(target_os = "macos")]
#[path = "cursor_export/platform_macos.rs"]
mod platform;
#[cfg(not(target_os = "macos"))]
#[path = "cursor_export/platform_unsupported.rs"]
mod platform;

pub(super) struct CursorExportRequest<'a> {
  pub audio_layout: AudioLayout,
  pub camera: Option<(&'a Path, BakedVideoExportOptions)>,
  pub cancelled: &'a AtomicBool,
  pub cursor: &'a Path,
  pub cursor_effects: CursorEffectSettings,
  pub destination: &'a Path,
  pub duration_ms: u64,
  pub height: u32,
  pub on_progress: &'a mut dyn FnMut(u64),
  pub screen: &'a Path,
  pub selection: &'a TrackSelection,
  pub video: VideoExportOptions,
  pub width: u32,
}

fn output_dimensions(width: u32, height: u32, video: VideoExportOptions) -> (u32, u32) {
  let scale = u64::from(video.resolution_scale_percent);
  let source_scale = u64::from(video.source_scale_percent.max(1));
  let scaled = |value: u32| ((u64::from(value) * scale / source_scale) as u32 & !1).max(2);
  (scaled(width), scaled(height))
}

fn video_bitrate(width: u32, height: u32, compression: u8) -> u64 {
  let quality = [0.05, 0.032, 0.012, 0.007, 0.004]
    .get(compression as usize)
    .copied()
    .unwrap_or(0.004);
  let pixels_per_second = f64::from(width) * f64::from(height) * 60.0;
  (pixels_per_second * quality).round().max(1_000_000.0) as u64
}

pub(super) fn estimated_video_bytes(
  width: u32,
  height: u32,
  duration_ms: u64,
  video: VideoExportOptions,
) -> u64 {
  let (width, height) = output_dimensions(width, height, video);
  video_bitrate(width, height, video.compression).saturating_mul(duration_ms) / 8_000
}

pub(super) fn export(request: CursorExportRequest<'_>) -> Result<ExportRunResult, String> {
  if !request.cursor_effects.bake {
    return Err("Cursor baking was not enabled".to_owned());
  }
  if !request.cursor_effects.size_percent.is_finite()
    || !(50.0..=500.0).contains(&request.cursor_effects.size_percent)
  {
    return Err("The cursor size is not valid".to_owned());
  }
  if !media_preview::supports_compression() {
    return Err("This FFmpeg build cannot finish the recording export".to_owned());
  }
  platform::export(request)
}
