// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};
use tauri::{LogicalPosition, LogicalSize};

/// Frame rates the bar offers.
pub(super) const DEFAULT_FPS: u32 = 60;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingStatus {
  #[default]
  Idle,
  Starting,
  Recording,
  Paused,
  Stopping,
}

impl RecordingStatus {
  pub(super) const fn label(self) -> &'static str {
    match self {
      Self::Idle => "idle",
      Self::Starting => "starting",
      Self::Recording => "recording",
      Self::Paused => "paused",
      Self::Stopping => "stopping",
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordingMode {
  Screen,
  Region,
  Window,
  Camera,
  Audio,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Region {
  pub position: LogicalPosition<f64>,
  pub size: LogicalSize<f64>,
}

/// Options assembled by the recording bar from the source and input stores.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRecordingOptions {
  pub mode: RecordingMode,
  #[serde(default)]
  pub monitor_id: Option<u32>,
  #[serde(default)]
  pub window_id: Option<u32>,
  #[serde(default)]
  pub region: Option<Region>,
  #[serde(default)]
  pub show_cursor: bool,
  #[serde(default)]
  pub system_audio: bool,
  #[serde(default)]
  pub system_audio_application_ids: Vec<String>,
  #[cfg(target_os = "windows")]
  #[serde(default)]
  pub system_audio_process_ids: Vec<u32>,
  #[serde(default)]
  pub microphone_id: Option<String>,
  #[serde(default)]
  pub camera_id: Option<String>,
  #[serde(default = "default_fps")]
  pub fps: u32,
}

/// A source snapshot taken when Record is pressed. Bundle identifiers resolve
/// ScreenCaptureKit applications on macOS; process identifiers are retained
/// alongside them for the eventual WASAPI implementation on Windows.
#[derive(Clone, Debug, Default)]
pub struct SystemAudioSelection {
  pub application_ids: Vec<String>,
  pub enabled: bool,
  #[cfg(target_os = "windows")]
  pub process_ids: Vec<u32>,
}

const fn default_fps() -> u32 {
  DEFAULT_FPS
}

/// Epoch-millisecond timestamps are stamped by Rust so every window - including
/// ones that reload or join late - derives the same elapsed time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSnapshot {
  pub status: RecordingStatus,
  pub mode: Option<RecordingMode>,
  pub started_at_ms: Option<u64>,
  pub accumulated_ms: u64,
  pub paused_at_ms: Option<u64>,
}
