// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native, bounded playback for the export window.
//!
//! Rust owns decode, audio output, seeking and the playback clock, and sends
//! the UI individual JPEG frames to draw on canvases.

use std::{path::PathBuf, sync::Mutex};

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager};

mod audio;
pub(crate) mod commands;
mod layout;
#[cfg(target_os = "macos")]
mod still_macos;
mod video;
#[cfg(target_os = "macos")]
mod video_macos;
mod worker;

use self::layout::{preview_layout, RecordingPreviewLayout, PREVIEW_HEIGHT};
#[cfg(target_os = "macos")]
use self::still_macos::NativeStillDecoder;
use self::worker::{PlaybackMode, PreviewPlayerWorker};
use super::{ExportArtifact, ExportState, RecordingAudioTrack};
pub use commands::stop_all;

#[derive(Clone)]
pub(super) struct PlayerSources {
  audio_tracks: Vec<RecordingAudioTrack>,
  camera_duration_ms: Option<u64>,
  camera_path: Option<PathBuf>,
  duration_ms: u64,
  layout: RecordingPreviewLayout,
  playback_layout: RecordingPreviewLayout,
  screen_path: PathBuf,
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
  event_channel: Option<Channel<RecordingPreviewPlayerEvent>>,
  frame_channel: Option<Channel>,
  is_playing: bool,
  latest_session_id: u64,
  latest_seek_request: u64,
  position_ms: u64,
  sources: Option<PlayerSources>,
  session_id: Option<u64>,
  #[cfg(target_os = "macos")]
  still_decoder: Option<NativeStillDecoder>,
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
    let frame_channel = self
      .frame_channel
      .clone()
      .ok_or_else(|| "The recording preview frame channel is unavailable".to_owned())?;
    let event_channel = self
      .event_channel
      .clone()
      .ok_or_else(|| "The recording preview event channel is unavailable".to_owned())?;
    #[cfg(target_os = "macos")]
    if matches!(mode, PlaybackMode::Still) {
      if self.still_decoder.is_none() {
        self.still_decoder = Some(NativeStillDecoder::spawn(
          sources,
          frame_channel,
          event_channel,
        )?);
      }
      return self
        .still_decoder
        .as_ref()
        .ok_or_else(|| "The native preview decoder is unavailable".to_owned())?
        .seek(self.position_ms, self.latest_seek_request, false);
    }
    self.worker = Some(PreviewPlayerWorker::spawn(
      sources,
      self.audio_indices.clone(),
      self.position_ms,
      self.latest_seek_request,
      mode,
      frame_channel,
      event_channel,
    )?);
    Ok(())
  }

  fn stop(&mut self) {
    self.cancel_worker();
    #[cfg(target_os = "macos")]
    if let Some(decoder) = self.still_decoder.take() {
      decoder.stop();
    }
    self.artifact_id = None;
    self.event_channel = None;
    self.frame_channel = None;
    self.is_playing = false;
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

fn sources(app: &AppHandle, artifact_id: u64) -> Result<PlayerSources, String> {
  let state = app.state::<ExportState>();
  let artifact = state
    .artifact
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  let Some(ExportArtifact::Recording {
    audio_tracks,
    camera,
    duration_ms,
    height,
    id,
    path,
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
  Ok(PlayerSources {
    audio_tracks: audio_tracks.clone(),
    camera_duration_ms: camera.as_ref().map(|value| value.duration_ms),
    camera_path: camera.as_ref().map(|value| value.path.clone()),
    duration_ms: *duration_ms,
    layout: preview_layout((*width, *height), camera_size, *height),
    playback_layout: preview_layout((*width, *height), camera_size, PREVIEW_HEIGHT),
    screen_path: path.clone(),
  })
}

#[cfg(test)]
mod tests {
  use super::layout::PreviewPaneKind;
  use super::*;

  #[test]
  fn lays_out_a_screen_as_one_native_preview_pane() {
    let layout = preview_layout((3_600, 2_338), None, PREVIEW_HEIGHT);

    assert_eq!(layout.panes.len(), 1);
    assert!(matches!(layout.panes[0].kind, PreviewPaneKind::Screen));
    assert_eq!(layout.panes[0].x, 0);
    assert_eq!(layout.width, layout.panes[0].width);
  }

  #[test]
  fn keeps_screen_and_portrait_camera_as_separate_panes() {
    let layout = preview_layout((3_600, 2_338), Some((1_080, 1_920)), PREVIEW_HEIGHT);

    assert_eq!(layout.panes.len(), 2);
    assert!(matches!(layout.panes[0].kind, PreviewPaneKind::Screen));
    assert!(matches!(layout.panes[1].kind, PreviewPaneKind::Camera));
    assert_eq!(layout.panes[1].x, layout.panes[0].width);
    assert_eq!(layout.width, layout.panes[0].width + layout.panes[1].width);
    assert!(layout.panes[1].width < layout.panes[0].width);
  }
}
