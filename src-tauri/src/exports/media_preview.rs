// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Audio prepared only for the export window.
//!
//! The recording remains the source of truth. FFmpeg decodes a low-rate mono
//! signal from each track for a waveform. Playback itself stays native and
//! decodes the selected tracks on demand without writing preview derivatives.
//!
//! Each track was once also stream-copied into its own small M4A. Nothing ever
//! played them - the waveforms are decoded straight from the recording and the
//! window plays the mix - so they were an FFmpeg pass and a file per track for
//! no reader at all.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

use super::track_selection::{AudioLayout, TrackSelection};
mod audio;
mod bake;
mod encode;
mod estimate;
mod output;
mod tools;
#[cfg(target_os = "windows")]
mod windows;

pub use audio::prepare;
#[cfg(target_os = "macos")]
pub(in crate::exports) use bake::bake_geometry;
#[cfg(target_os = "macos")]
pub(in crate::exports) use encode::remux_error;
pub use encode::{
  camera_recording_exporter, remuxer, selected_audio_exporter, selected_recording_exporter, Remux,
  SelectedRecordingExport,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(in crate::exports) use encode::{remux_temp_path, run_export};
pub use estimate::{estimate_compressed_video_bytes, supports_compression};
pub use output::duration_ms;
pub use tools::inspect_audio_tracks;
#[cfg(target_os = "windows")]
pub(in crate::exports) use windows::recording_info;

pub(in crate::exports) use estimate::export_crf;
use estimate::resolution_filter;
use output::{holds_bytes, plays_from_start_to_end, EXPORT_MP4_OUTPUT, OUTPUT_ERROR_DETAIL};
pub(crate) use tools::ffmpeg_path;
use tools::ffprobe_path;

use super::{AudioTrackKind, RecordingAudioTrack};

const WAVEFORM_POINTS: usize = 512;
const WAVEFORM_SAMPLE_RATE: u64 = 8_000;
/// Every file this module writes starts with it. Nothing else in the
/// recordings directory does, which is what lets both the cleanup paths and
/// the startup sweep tell a derivative from a recording by its name alone.
pub const PREVIEW_PREFIX: &str = "preview-";

/// Whether a path is one of this module's derivatives rather than a recording.
pub fn is_preview_file(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .is_some_and(|name| name.starts_with(PREVIEW_PREFIX))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAudioTrack {
  pub kind: AudioTrackKind,
  pub label: String,
  /// Which recorded track this describes, so the window can name it back when
  /// it asks for a mix. Also what identifies the row on screen.
  pub stream_index: usize,
  pub waveform: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreview {
  pub artifact_id: u64,
  pub tracks: Vec<PreparedAudioTrack>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoExportOptions {
  pub compression: u8,
  pub resolution_scale_percent: u16,
  pub source_scale_percent: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedVideoExportOptions {
  pub camera_height: u32,
  pub camera_width: u32,
  pub overlay: super::CameraOverlaySettings,
  pub screen_height: u32,
  pub screen_width: u32,
  pub video: VideoExportOptions,
}

pub struct ExportRunOptions<'a> {
  pub cancelled: &'a AtomicBool,
  pub on_progress: &'a mut dyn FnMut(u64),
  pub video: VideoExportOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportRunResult {
  Completed,
  Cancelled,
}

#[cfg(test)]
mod tests;
