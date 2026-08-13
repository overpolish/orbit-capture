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
mod platform;
pub(crate) mod surface_commands;
pub(crate) mod timeline_thumbnails;
mod video;
mod worker;

use self::layout::{preview_layout, RecordingPreviewLayout};
use self::worker::{PlaybackMode, PreviewPlayerWorker};
use super::preview_platform::RecordingPreviewSurface;
use super::{
  cursor_effects::{CursorCompositor, CursorEffectSettings},
  AudioTrackVolume, CameraOverlaySettings, ExportArtifact, ExportState, RecordingAudioTrack,
  RecordingOutputSettings,
};
use crate::recording::PrimaryRecordingKind;
pub use commands::stop_all;

#[derive(Clone)]
pub(super) struct PlayerSources {
  audio_tracks: Vec<RecordingAudioTrack>,
  camera_duration_ms: Option<u64>,
  camera_path: Option<PathBuf>,
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  cursor: Option<Arc<CursorCompositor>>,
  cursor_settings: Arc<RwLock<CursorEffectSettings>>,
  composition_settings: Option<Arc<RwLock<PreviewCompositionSettings>>>,
  duration_ms: u64,
  /// Zero when OSCs are hidden, one for the primary pane and two for camera.
  layout: RecordingPreviewLayout,
  playback_layout: RecordingPreviewLayout,
  /// True while real-time playback owns the surface, so a late still decode
  /// never stomps a playing frame.
  playing: Arc<AtomicBool>,
  preview_surface: Option<Arc<RecordingPreviewSurface>>,
  screen_path: PathBuf,
}

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

#[derive(Clone, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct PreviewCompositionSettings {
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  recording_output: RecordingOutputSettings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordingPreviewPlayerInfo {
  pub duration_ms: u64,
  pub layout: RecordingPreviewLayout,
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
  frame_channel: Option<Channel>,
  is_playing: bool,
  latest_session_id: u64,
  latest_layout_request: u64,
  latest_seek_request: u64,
  pane_target_sizes: Vec<(u32, u32)>,
  position_ms: u64,
  /// The next still seek came from a scrub gesture in progress, so the
  /// scrubber may land on the cheapest nearby frame for immediacy.
  rough_seek: bool,
  sources: Option<PlayerSources>,
  session_id: Option<u64>,
  still_decoder: Option<platform::StillDecoder>,
  worker: Option<PreviewPlayerWorker>,
}

impl PreviewPlayerManager {
  fn cancel_worker(&mut self) {
    if let Some(worker) = self.worker.take() {
      self.position_ms = worker.cancel();
    }
  }

  fn restart(&mut self, mode: PlaybackMode) -> Result<(), String> {
    self.cancel_worker();
    let sources = self
      .sources
      .clone()
      .ok_or_else(|| "The recording preview player is not open".to_owned())?;
    sources
      .playing
      .store(matches!(mode, PlaybackMode::Playing), Ordering::Release);
    if matches!(mode, PlaybackMode::Still) {
      if let Some(surface) = &sources.preview_surface {
        surface.hide();
      }
    }
    let frame_channel = self
      .frame_channel
      .clone()
      .ok_or_else(|| "The recording preview frame channel is unavailable".to_owned())?;
    let event_channel = self
      .event_channel
      .clone()
      .ok_or_else(|| "The recording preview event channel is unavailable".to_owned())?;
    if platform::NATIVE_STILLS
      && matches!(mode, PlaybackMode::Still | PlaybackMode::InteractiveStill)
      && !sources.layout.panes.is_empty()
    {
      let rough = std::mem::take(&mut self.rough_seek);
      if self.still_decoder.is_none() {
        self.still_decoder = Some(platform::StillDecoder::spawn(sources, event_channel)?);
      }
      return self
        .still_decoder
        .as_ref()
        .ok_or_else(|| "The native preview decoder is unavailable".to_owned())?
        .seek(
          self.position_ms,
          self.latest_seek_request,
          rough,
          self.pane_target_sizes.clone(),
        );
    }
    self.rough_seek = false;
    let playback_factors = self.playback_factors(&sources);
    self.worker = Some(PreviewPlayerWorker::spawn(
      sources,
      worker::WorkerLaunch {
        audio: PreviewAudioSettings {
          audio_track_volumes: self.audio_volumes.clone(),
          enabled_stream_indices: self.audio_indices.clone(),
        },
        mode,
        playback_factors,
        request_id: self.latest_seek_request,
        start_ms: self.position_ms,
      },
      frame_channel,
      event_channel,
    )?);
    Ok(())
  }

  /// How much each pane's playback decode shrinks to match the on-screen pane
  /// size, mirroring what the still decoder presents.
  fn playback_factors(&self, sources: &PlayerSources) -> Vec<f64> {
    platform::playback_factors(&self.pane_target_sizes, sources)
  }

  fn stop(&mut self) {
    self.cancel_worker();
    if let Some(sources) = self.sources.as_ref() {
      sources.playing.store(false, Ordering::Release);
      if let Some(surface) = sources.preview_surface.as_ref() {
        surface.hide();
      }
    }
    if let Some(decoder) = self.still_decoder.take() {
      decoder.stop();
    }
    self.artifact_id = None;
    self.event_channel = None;
    self.frame_channel = None;
    self.is_playing = false;
    self.pane_target_sizes.clear();
    self.sources = None;
    self.session_id = None;
  }

  fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That recording preview player session is no longer active".to_owned())
  }
}

#[derive(Default)]
pub struct RecordingPreviewPlayerState(Mutex<PreviewPlayerManager>);

fn sources(
  app: &AppHandle,
  artifact_id: u64,
  settings: Option<&PreviewPlayerSettings>,
) -> Result<PlayerSources, String> {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(ExportArtifact::Recording {
    audio_tracks,
    camera,
    cursor,
    duration_ms,
    height,
    id,
    path,
    primary_kind,
    width,
    ..
  }) = artifact.as_ref()
  else {
    return Err("There is no recording to preview".to_owned());
  };
  if *id != artifact_id {
    return Err("That recording is no longer waiting to be exported".to_owned());
  }
  let camera_size = camera.as_ref().map(|value| (value.width, value.height));
  let primary_pane = match primary_kind {
    PrimaryRecordingKind::Screen => Some((*width, *height, layout::PreviewPaneKind::Screen)),
    PrimaryRecordingKind::Camera => Some((*width, *height, layout::PreviewPaneKind::Camera)),
    PrimaryRecordingKind::Audio => None,
  };
  let preview_surface = app
    .get_webview_window("export")
    .map(|window| RecordingPreviewSurface::from_window(&window).map(Arc::new))
    .transpose()?;
  let layout = preview_layout(primary_pane, camera_size, *height);
  // Native playback decodes every source at its own stored resolution. The
  // presentation surface handles its visual size, so a portrait camera is not
  // needlessly enlarged to the screen track's height before composition.
  let mut playback_layout = layout.clone();
  for pane in &mut playback_layout.panes {
    pane.width = pane.source_width;
    pane.height = pane.source_height;
  }
  playback_layout.width = playback_layout.panes.iter().map(|pane| pane.width).sum();
  playback_layout.height = playback_layout
    .panes
    .iter()
    .map(|pane| pane.height)
    .max()
    .unwrap_or(0);
  Ok(PlayerSources {
    audio_tracks: audio_tracks.clone(),
    camera_duration_ms: camera.as_ref().map(|value| value.duration_ms),
    camera_path: camera.as_ref().map(|value| value.path.clone()),
    cursor: cursor
      .as_ref()
      .map(|value| CursorCompositor::open(&value.path).map(Arc::new))
      .transpose()?,
    composition_settings: settings.map(|settings| {
      Arc::new(RwLock::new(PreviewCompositionSettings {
        bake_camera: settings.bake_camera,
        camera_overlay: settings.camera_overlay,
        recording_output: settings.recording_output.clone(),
      }))
    }),
    cursor_settings: Arc::new(RwLock::new(
      settings.map_or_else(CursorEffectSettings::default, |settings| {
        settings.cursor_effects
      }),
    )),
    duration_ms: *duration_ms,
    layout,
    playback_layout,
    playing: Arc::new(AtomicBool::new(false)),
    preview_surface,
    screen_path: path.clone(),
  })
}
