// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

mod artifact;
mod audio_save;
mod camera_save;
pub(crate) mod commands;
mod cursor_effects;
mod cursor_export;
mod directory;
mod media_preview;
mod naming;
mod preferences;
pub(crate) mod preview;
pub(crate) mod preview_platform;
pub(crate) mod recording_preview;
pub(crate) mod recording_preview_player;
mod recovery;
pub(crate) mod save;
pub(crate) mod screenshot_preview;
mod track_selection;
mod validation;
mod workspace;

pub use artifact::{discard, present_recording, present_screenshot};
use artifact::{emit_snapshot, full_preview_png, screenshot_image, snapshot, take_artifact};
use camera_save::validate_camera_overlay;
use commands::set_export_directory;
use directory::current_directory;
#[cfg(target_os = "windows")]
pub(crate) use media_preview::ffmpeg_path;
use naming::sanitize_file_stem;
use preferences::{
  load_cursor_effects, load_recording_output, load_screenshot_background_radius,
  load_screenshot_output, load_screenshot_radius, remember_completed_export,
  remember_screenshot_background_radius, remember_screenshot_output, remember_screenshot_radius,
};
pub use recovery::initialize;
#[cfg(test)]
use recovery::orphan_plan;
use save::{delivered_extension, scale_percent};
use validation::{validate_camera_resolution_scale, validate_primary_resolution_scale};
pub use workspace::{
  focus_if_pending as focus_pending_workspace,
  focus_if_screenshot_blocked as focus_if_screenshot_workspace_blocked,
  has_pending as has_pending_workspace, release_recording as release_recording_workspace,
  release_screenshot as release_screenshot_workspace,
  reserve_recording as reserve_recording_workspace,
  reserve_screenshot as reserve_screenshot_workspace,
};
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

use crate::recording::{FinalizeInfo, PrimaryRecordingKind};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use crate::screenshots::compose_screenshot;
use crate::screenshots::{
  encode_png, screenshot_directory, unique_path, CapturedImage, ScreenshotOutputSettings,
};

#[cfg(target_os = "macos")]
pub(crate) fn initialize_cursor_artwork() {
  cursor_effects::initialize_artwork();
}

const EXPORT_CHANGED_EVENT: &str = "export://artifact";
const EXPORT_PROGRESS_EVENT: &str = "export://progress";
const EXPORT_PREFERENCES_FILE: &str = "export-preferences.json";
const SCREENSHOT_EXTENSION: &str = "png";
/// What a saved recording is delivered as when it can be, which is whenever
/// FFmpeg is on the machine. See [`save_recording`] for the other case.
const RECORDING_EXTENSION: &str = "mp4";
const AUDIO_EXTENSION: &str = "m4a";
/// The container a recording is written to while it runs. macOS writes a
/// fragmented QuickTime movie; Windows writes fragmented MP4. Both retain
/// completed fragments if the app dies mid-recording.
const WORKING_RECORDING_EXTENSION: &str = if cfg!(windows) { "mp4" } else { "mov" };
/// Every extension a working recording can be found under in the recordings
/// directory. `.mp4` is there for the files an earlier version of the app left
/// behind: an upgrade must not walk past someone's unsaved recording.
const WORKING_RECORDING_EXTENSIONS: &[&str] = &["mov", "mp4"];
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
    /// Ordered back-to-front. The first slice keeps the existing single-item
    /// compositor contract while the native scene renderer is introduced.
    items: Vec<ScreenshotItem>,
    suggested_file_stem: String,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    camera: Option<RecordingCamera>,
    cursor: Option<RecordingCursor>,
    id: u64,
    duration_ms: u64,
    height: u32,
    /// The working file. Saving moves it or derives the requested compressed
    /// copy; discarding deletes it.
    path: PathBuf,
    primary_kind: PrimaryRecordingKind,
    source_scale_percent: u16,
    suggested_file_stem: String,
    width: u32,
  },
}

/// One independently editable image in a screenshot workspace.
///
/// Pixels remain owned by Rust and are uploaded to the native renderer once;
/// the webview only ever needs this identity and the scene metadata added in
/// the next slice.
#[derive(Clone)]
pub struct ScreenshotItem {
  pub id: u64,
  pub image: CapturedImage,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotWorkspaceItemOutput {
  pub id: u64,
  pub output: ScreenshotOutputSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotWorkspaceOutputSettings {
  #[serde(flatten)]
  pub canvas: ScreenshotOutputSettings,
  #[serde(default)]
  pub items: Vec<ScreenshotWorkspaceItemOutput>,
}

impl ScreenshotWorkspaceOutputSettings {
  pub(super) fn output_for(&self, item: &ScreenshotItem) -> ScreenshotOutputSettings {
    self.output_for_id(item.id)
  }

  pub(super) fn output_for_id(&self, id: u64) -> ScreenshotOutputSettings {
    let mut output = self
      .items
      .iter()
      .find(|candidate| candidate.id == id)
      .map_or_else(|| self.canvas.clone(), |candidate| candidate.output.clone());
    output.background_color = self.canvas.background_color.clone();
    output.background_type = self.canvas.background_type.clone();
    output.background_radius_percent = self.canvas.background_radius_percent;
    output.height = self.canvas.height;
    output.mesh_colors = self.canvas.mesh_colors.clone();
    output.mesh_locked_colors = self.canvas.mesh_locked_colors.clone();
    output.mesh_points = self.canvas.mesh_points.clone();
    output.mesh_seed = self.canvas.mesh_seed;
    output.mesh_warp_percent = self.canvas.mesh_warp_percent;
    output.width = self.canvas.width;
    output
  }
}

fn compose_screenshot_workspace(
  app: &AppHandle,
  items: &[ScreenshotItem],
  output: &ScreenshotWorkspaceOutputSettings,
) -> Result<CapturedImage, String> {
  #[cfg(not(target_os = "windows"))]
  let _ = app;
  let ordered_items = output
    .items
    .iter()
    .filter_map(|item_output| items.iter().find(|item| item.id == item_output.id))
    .collect::<Vec<_>>();
  #[cfg(not(target_os = "windows"))]
  let first = ordered_items
    .first()
    .copied()
    .ok_or_else(|| "The screenshot workspace is empty".to_owned())?;
  #[cfg(target_os = "macos")]
  {
    let mut composed = crate::screenshots::compose_output_layers(
      &first.image,
      &output.output_for(first),
      0.0,
      true,
      None,
      None,
      None,
      false,
      false,
    )?;
    for item in &ordered_items[1..] {
      let layer = crate::screenshots::compose_output_layers(
        &item.image,
        &output.output_for(item),
        0.0,
        true,
        None,
        None,
        None,
        false,
        true,
      )?;
      composed = crate::screenshots::alpha_composite(&composed, &layer)?;
    }
    Ok(composed)
  }
  #[cfg(target_os = "windows")]
  {
    let window = app
      .get_webview_window(crate::windows::WindowLabel::Export.as_str())
      .ok_or_else(|| "The export window is unavailable".to_owned())?;
    let surface = preview_platform::RecordingPreviewSurface::from_window(&window)?;
    let layers = ordered_items
      .iter()
      .map(|item| (&item.image, output.output_for(item)))
      .collect::<Vec<_>>();
    surface.compose_screenshot_layers_to_image(&layers)
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  {
    compose_screenshot(&first.image, &output.output_for(first))
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotItemSnapshot {
  pub height: u32,
  pub id: u64,
  pub width: u32,
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
    items: Vec<ScreenshotItemSnapshot>,
    suggested_file_stem: String,
    extension: String,
    width: u32,
    height: u32,
  },
  Recording {
    audio_tracks: Vec<RecordingAudioTrack>,
    camera: Option<RecordingCamera>,
    can_compress: bool,
    cursor_data_version: Option<u16>,
    has_cursor_data: bool,
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
    primary_kind: PrimaryRecordingKind,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingCamera {
  pub duration_ms: u64,
  pub height: u32,
  pub original_size_bytes: u64,
  pub path: PathBuf,
  pub width: u32,
}

pub struct RecordingCursor {
  pub format_version: u16,
  pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CameraOverlaySettings {
  pub camera_x_percent: f64,
  pub camera_y_percent: f64,
  pub camera_width_percent: f64,
  pub frame_height_percent: f64,
  pub frame_width_percent: f64,
  pub frame_x_percent: f64,
  pub frame_y_percent: f64,
  pub radius_percent: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecordingExportOptions {
  pub audio_track_volumes: Vec<AudioTrackVolume>,
  pub bake_camera: bool,
  pub camera_compression: u8,
  pub camera_overlay: CameraOverlaySettings,
  pub camera_resolution_scale_percent: u16,
  pub collapse_audio: bool,
  pub compression: u8,
  pub cursor_effects: cursor_effects::CursorEffectSettings,
  pub enabled_stream_indices: Vec<usize>,
  pub include_camera: bool,
  pub include_primary_video: bool,
  pub resolution_scale_percent: u16,
  pub recording_output: RecordingOutputSettings,
  pub screenshot_output: ScreenshotWorkspaceOutputSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingOutputSettings {
  pub camera: ScreenshotOutputSettings,
  #[serde(default = "default_camera_on_top")]
  pub camera_on_top: bool,
  pub primary: ScreenshotOutputSettings,
}

fn default_camera_on_top() -> bool {
  true
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrackVolume {
  pub decibels: i16,
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

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
  pub artifact: Option<ExportArtifactSnapshot>,
  pub cursor_effects: cursor_effects::CursorEffectSettings,
  pub directory: Option<PathBuf>,
  pub recording_output: Option<RecordingOutputSettings>,
  pub screenshot_radius_percent: f64,
  pub screenshot_background_radius_percent: f64,
  pub screenshot_output: Option<ScreenshotOutputSettings>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportProgress {
  artifact_id: u64,
  phase: &'static str,
  progress_percent: f64,
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
  capture_reservation: Mutex<Option<workspace::CaptureWorkspaceReservation>>,
  generation: AtomicU64,
  preview: Mutex<Option<Vec<u8>>>,
  /// Built only if the user zooms in, because it is the whole capture.
  full_preview: Mutex<Option<Vec<u8>>>,
  directory: Mutex<Option<PathBuf>>,
  cursor_effects: Mutex<cursor_effects::CursorEffectSettings>,
  recording_output: Mutex<Option<RecordingOutputSettings>>,
  screenshot_radius_percent: Mutex<f64>,
  screenshot_background_radius_percent: Mutex<f64>,
  screenshot_output: Mutex<Option<ScreenshotOutputSettings>>,
  recording_preview: Mutex<Option<media_preview::RecordingPreview>>,
  recording_preview_preparation: Mutex<()>,
  /// Cached by artifact, stream kind (screen/camera/baked), quality and scale.
  compression_estimates: Mutex<HashMap<(u64, u8, u8, u16), u64>>,
  compression_estimate_preparation: Mutex<()>,
}

#[cfg(test)]
mod tests;
