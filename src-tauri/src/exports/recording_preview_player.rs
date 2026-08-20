// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native, bounded playback for the export window.
//!
//! Rust owns decode, audio output, seeking and the playback clock. Native
//! platform surfaces own video presentation; the webview only supplies layout
//! and interaction state.

use std::{
  path::PathBuf,
  sync::{atomic::AtomicBool, atomic::Ordering, Arc, Mutex, RwLock},
};

use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, Manager};

mod audio;
pub(crate) mod commands;
mod layout;
mod lifecycle;
mod platform;
mod selection_gesture;
mod selection_preview;
mod sources;
#[cfg(target_os = "windows")]
pub(crate) use platform::GpuVideoReader;
pub(crate) mod surface_commands;
pub(crate) mod timeline_thumbnails;
mod video;
mod worker;

use self::layout::{preview_layout, RecordingPreviewLayout};
use self::sources::{sources, PlayerSources};
use self::worker::{PlaybackMode, PreviewPlayerWorker};
use super::preview_platform::workspace_editor::WorkspaceScene;
use super::preview_platform::RecordingPreviewSurface;
use super::{
  cursor_effects::{CursorCompositor, CursorEffectSettings},
  AudioTrackVolume, CameraOverlaySettings, ExportArtifact, ExportKind, ExportState,
  RecordingAudioTrack, RecordingOutputSettings,
};
use crate::recording::PrimaryRecordingKind;
pub use commands::stop_all;

pub(super) const AUTO_FIT_MOVE_EDGE: u32 = 1 << 17;
const AUTO_FIT_COMMIT_EDGE: u32 = 1 << 18;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewAudioSettings {
  pub audio_track_volumes: Vec<AudioTrackVolume>,
  pub enabled_stream_indices: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewPlayerSettings {
  pub audio: PreviewAudioSettings,
  pub bake_camera: bool,
  pub camera_overlay: CameraOverlaySettings,
  pub cursor_effects: CursorEffectSettings,
  pub recording_output: RecordingOutputSettings,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct PreviewCompositionSettings {
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  recording_output: RecordingOutputSettings,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct RecordingSelectionGesture {
  snapshot: PreviewCompositionSettings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingPreviewPlayerInfo {
  pub duration_ms: u64,
  pub layout: RecordingPreviewLayout,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingPreviewTransformEvent {
  session_id: u64,
  zoom_percent: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
  rename_all = "camelCase",
  rename_all_fields = "camelCase",
  tag = "event",
  content = "data"
)]
pub enum RecordingPreviewPlayerEvent {
  Ended,
  Error { message: String },
  Paused { position_ms: u64 },
  Playing { position_ms: u64 },
  Position { position_ms: u64 },
  Ready { position_ms: u64, request_id: u64 },
}

#[derive(Default)]
struct PreviewPlayerManager {
  artifact_id: Option<u64>,
  audio_indices: Vec<usize>,
  audio_volumes: Vec<AudioTrackVolume>,
  event_channel: Option<Channel<RecordingPreviewPlayerEvent>>,
  is_playing: bool,
  latest_session_id: u64,
  latest_layout_request: u64,
  latest_seek_request: u64,
  pane_target_sizes: Vec<(u32, u32)>,
  position_ms: u64,
  /// The next still seek came from a scrub gesture in progress, so the
  /// scrubber may land on the cheapest nearby frame for immediacy.
  rough_seek: bool,
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  selection_gesture: Option<RecordingSelectionGesture>,
  sources: Option<PlayerSources>,
  session_id: Option<u64>,
  still_decoder: Option<platform::StillDecoder>,
  workspace_scene: Option<WorkspaceScene>,
  worker: Option<PreviewPlayerWorker>,
}

#[derive(Default)]
pub struct RecordingPreviewPlayerState(Mutex<PreviewPlayerManager>);
