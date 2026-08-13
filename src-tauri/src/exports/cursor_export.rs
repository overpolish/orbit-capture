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
use crate::screenshots::ScreenshotOutputSettings;

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
  pub audio_source: Option<&'a Path>,
  pub camera: Option<(&'a Path, BakedVideoExportOptions)>,
  pub cancelled: &'a AtomicBool,
  pub cursor: Option<&'a Path>,
  pub cursor_effects: CursorEffectSettings,
  pub destination: &'a Path,
  pub duration_ms: u64,
  pub height: u32,
  pub on_progress: &'a mut dyn FnMut(u64),
  pub screen: &'a Path,
  pub selection: &'a TrackSelection,
  pub output: &'a ScreenshotOutputSettings,
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
  // Bits per pixel per frame. Mesh gradient backgrounds are the hardest
  // content H.264 sees here: starving them visibly bands and blocks, so the
  // ladder is sized for smooth gradients over text rather than flat UI.
  let quality = [0.1, 0.065, 0.036, 0.02, 0.011]
    .get(compression as usize)
    .copied()
    .unwrap_or(0.011);
  // Perceptual quality does not scale linearly with resolution: a downscaled
  // export needs more bits per pixel than a native-resolution one to look
  // equally clean, so smaller outputs get a gentle density boost relative to
  // a 4K reference.
  let pixels = f64::from(width) * f64::from(height);
  let density_boost = (8_294_400.0 / pixels.max(1.0)).powf(0.25).clamp(1.0, 1.8);
  let pixels_per_second = pixels * 60.0;
  (pixels_per_second * quality * density_boost)
    .round()
    .max(2_000_000.0) as u64
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
  if request.cursor.is_some() && !request.cursor_effects.bake {
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

pub(super) fn needs_composition(
  settings: &ScreenshotOutputSettings,
  source_width: u32,
  source_height: u32,
) -> bool {
  settings.width != source_width
    || settings.height != source_height
    || settings.background_radius_percent > 0.0
    || settings.radius_percent > 0.0
    || (settings.screenshot_crop_height_percent - 100.0).abs() > 0.000_001
    || (settings.screenshot_crop_width_percent - 100.0).abs() > 0.000_001
    || settings.screenshot_crop_x_percent.abs() > 0.000_001
    || settings.screenshot_crop_y_percent.abs() > 0.000_001
    || (settings.screenshot_image_width_percent - 100.0).abs() > 0.000_001
    || (settings.screenshot_image_x_percent - 50.0).abs() > 0.000_001
    || (settings.screenshot_image_y_percent - 50.0).abs() > 0.000_001
}
