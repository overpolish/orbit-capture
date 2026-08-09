// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod artifact;
pub(crate) mod commands;
mod directory;
mod media_preview;
mod naming;
pub(crate) mod preview;
mod recovery;
pub(crate) mod save;
mod track_selection;

pub use artifact::{discard, present_recording, present_screenshot};
use artifact::{emit_snapshot, full_preview_png, snapshot, take_artifact};
use commands::set_export_directory;
use directory::{current_directory, load_directory, store_directory};
use naming::sanitize_file_stem;
pub use recovery::initialize;
#[cfg(test)]
use recovery::orphan_plan;
use save::{delivered_extension, scale_percent, validate_resolution_scale};
mod window;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use tauri::{image::Image, ipc::Response, AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::recording::FinalizeInfo;
use crate::screenshots::{encode_png, screenshot_directory, unique_path, CapturedImage};

const EXPORT_CHANGED_EVENT: &str = "export://artifact";
const EXPORT_PROGRESS_EVENT: &str = "export://progress";
const EXPORT_DIRECTORY_FILE: &str = "export-directory.json";
const SCREENSHOT_EXTENSION: &str = "png";
/// What a saved recording is delivered as when it can be, which is whenever
/// FFmpeg is on the machine. See [`save_recording`] for the other case.
const RECORDING_EXTENSION: &str = "mp4";
/// The container a recording is written to while it runs. It is a QuickTime
/// movie because that is the only container that survives being fragmented,
/// and only a fragmented file is worth anything if the app dies mid-recording
/// - see `recording::platform::Container::quicktime_fragmented`.
const WORKING_RECORDING_EXTENSION: &str = "mov";
/// Every extension a working recording can be found under in the recordings
/// directory. `.mp4` is there for the files an earlier version of the app left
/// behind: an upgrade must not walk past someone's unsaved recording.
const WORKING_RECORDING_EXTENSIONS: &[&str] = &[WORKING_RECORDING_EXTENSION, RECORDING_EXTENSION];
/// How long an unclaimed recording is kept before it is swept away. Long
/// enough that a crash is recoverable, short enough that a forgotten one does
/// not sit in the app's data directory forever.
const ORPHAN_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// The long edge of the preview shipped to the window. The capture itself can
/// be 30 MB of pixels, which has no business crossing the IPC boundary.
const PREVIEW_MAX_EDGE: u32 = 640;
const MAX_FILE_STEM: usize = 200;

/// A capture waiting to be saved.
///
/// The window renders itself by artifact kind rather than assuming a
/// screenshot, because a recording is a file on disk rather than pixels in
/// memory and almost nothing about handling it is the same.
pub enum ExportArtifact {
  Screenshot {
    /// Unique per capture. Two consecutive fullscreen captures are identical
    /// in every other respect, so the window needs this to tell them apart
    /// and start the new one at fit rather than inheriting the old zoom.
    id: u64,
    image: CapturedImage,
    suggested_file_stem: String,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    id: u64,
    duration_ms: u64,
    height: u32,
    /// The working file. Saving moves it or derives the requested compressed
    /// copy; discarding deletes it.
    path: PathBuf,
    source_scale_percent: u16,
    suggested_file_stem: String,
    width: u32,
  },
}

/// What the window is told about the pending artifact. Deliberately without
/// pixels: the preview travels separately, as bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "kind"
)]
pub enum ExportArtifactSnapshot {
  Screenshot {
    id: u64,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    can_compress: bool,
    id: u64,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
    duration_ms: u64,
    original_size_bytes: u64,
    /// The working file, for the window to play through the asset protocol.
    /// Scoped to the recordings directory in `tauri.conf.json`, which is the
    /// only place this path can ever point.
    path: PathBuf,
    source_scale_percent: u16,
  },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioTrackKind {
  SystemAudio,
  Microphone,
  Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingAudioTrack {
  pub kind: AudioTrackKind,
  pub label: String,
  pub stream_index: usize,
}

fn recording_audio_tracks(
  has_system_audio: bool,
  has_microphone: bool,
) -> Vec<RecordingAudioTrack> {
  let mut tracks = Vec::with_capacity(usize::from(has_system_audio) + usize::from(has_microphone));
  if has_system_audio {
    tracks.push(RecordingAudioTrack {
      kind: AudioTrackKind::SystemAudio,
      label: "System audio".to_owned(),
      stream_index: tracks.len(),
    });
  }
  if has_microphone {
    tracks.push(RecordingAudioTrack {
      kind: AudioTrackKind::Microphone,
      label: "Microphone".to_owned(),
      stream_index: tracks.len(),
    });
  }
  tracks
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
  pub artifact: Option<ExportArtifactSnapshot>,
  pub directory: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
  artifact_id: u64,
  processed_ms: u64,
}

#[derive(Clone)]
struct ActiveExportJob {
  artifact_id: u64,
  cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct ExportState {
  active_export: Mutex<Option<ActiveExportJob>>,
  artifact: Mutex<Option<ExportArtifact>>,
  generation: AtomicU64,
  preview: Mutex<Option<Vec<u8>>>,
  /// Built only if the user zooms in, because it is the whole capture.
  full_preview: Mutex<Option<Vec<u8>>>,
  directory: Mutex<Option<PathBuf>>,
  recording_preview: Mutex<Option<media_preview::RecordingPreview>>,
  recording_preview_preparation: Mutex<()>,
  preview_mixes: Mutex<media_preview::PreviewMixes>,
  preview_mix_preparation: Mutex<()>,
  compression_estimates: Mutex<HashMap<(u64, u8, u16), u64>>,
  compression_estimate_preparation: Mutex<()>,
}

#[cfg(test)]
mod tests;
