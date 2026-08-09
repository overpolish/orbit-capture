// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Audio prepared only for the export window.
//!
//! The recording remains the source of truth. FFmpeg decodes a low-rate mono
//! signal from each of its tracks for a waveform, and mixes the enabled ones
//! into a single file the window can play. Closing or saving the artifact
//! removes the mixes; none of them can become an export by accident.
//!
//! Each track was once also stream-copied into its own small M4A. Nothing ever
//! played them - the waveforms are decoded straight from the recording and the
//! window plays the mix - so they were an FFmpeg pass and a file per track for
//! no reader at all.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

use super::track_selection::{AudioLayout, TrackSelection};
mod audio;
mod encode;
mod estimate;
mod preview_mix;
mod tools;

pub use audio::prepare;
pub use encode::{remuxer, selected_recording_exporter, Remux, SelectedRecordingExport};
pub use estimate::{estimate_compressed_video_bytes, supports_compression};
pub use preview_mix::{preview_mix, PreviewMixes};
pub use tools::inspect_audio_tracks;

use estimate::{export_crf, resolution_filter};
use preview_mix::{holds_bytes, plays_from_start_to_end, EXPORT_MP4_OUTPUT, MIX_ERROR_DETAIL};
use tools::{ffmpeg_path, ffprobe_path};

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
