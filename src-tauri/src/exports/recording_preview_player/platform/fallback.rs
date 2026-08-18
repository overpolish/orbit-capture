// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Preview backend for platforms without a native decode/composite path.
//!
//! FFmpeg decodes to MJPEG and the frames are sent to the webview, which draws
//! them into a canvas. This is the experience on any platform whose native
//! backend has not landed yet; see the parent module for what a real one owes.

mod thumbnails;
mod video;

use std::{
  process::Child,
  sync::{atomic::AtomicBool, mpsc::SyncSender, Arc, Mutex},
};

use tauri::ipc::{Channel, InvokeResponseBody};

use super::super::{video::VideoFrame, PlayerSources, RecordingPreviewPlayerEvent};

/// Paused frames come from [`spawn_video`] with `still = true`, so the player
/// never constructs a [`StillDecoder`].
pub(crate) const NATIVE_STILLS: bool = false;

pub(crate) enum VideoFramePayload {
  Composite(Vec<u8>),
}

/// Placeholder for the native paused-frame/scrub decoder. It exists so the
/// shared player can name the type unconditionally; because [`NATIVE_STILLS`]
/// is false it is never constructed, and every method is unreachable.
pub(crate) struct StillDecoder;

impl StillDecoder {
  pub(crate) fn spawn(
    _sources: PlayerSources,
    _event_channel: Channel<RecordingPreviewPlayerEvent>,
    _frame_channel: Channel,
  ) -> Result<Self, String> {
    Err("This platform has no native preview decoder".to_owned())
  }

  pub(crate) fn seek(
    &self,
    _position_ms: u64,
    _request_id: u64,
    _rough: bool,
    _target_sizes: Vec<(u32, u32)>,
  ) -> Result<(), String> {
    Err("This platform has no native preview decoder".to_owned())
  }

  pub(crate) fn stop(self) {}

  pub(crate) fn is_finished(&self) -> bool {
    false
  }
}

pub(crate) fn send_frame(
  channel: &Channel,
  sources: &PlayerSources,
  request_id: u64,
  payload: VideoFramePayload,
) -> bool {
  match payload {
    VideoFramePayload::Composite(bytes) => {
      let mut payload = Vec::with_capacity(16 + bytes.len());
      payload.extend_from_slice(&sources.playback_layout.width.to_le_bytes());
      payload.extend_from_slice(&sources.playback_layout.height.to_le_bytes());
      payload.extend_from_slice(&request_id.to_le_bytes());
      payload.extend_from_slice(&bytes);
      channel.send(InvokeResponseBody::Raw(payload)).is_ok()
    }
  }
}

/// `playback_factors` is unused here: FFmpeg is already asked for the pane size
/// through the playback layout.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_video(
  sources: &PlayerSources,
  _playback_factors: &[f64],
  start_ms: u64,
  still: bool,
  cancelled: Arc<AtomicBool>,
  child: Arc<Mutex<Option<Child>>>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  video::spawn(sources, start_ms, still, cancelled, child, sender)
}

pub(crate) fn playback_factors(
  _pane_target_sizes: &[(u32, u32)],
  sources: &PlayerSources,
) -> Vec<f64> {
  vec![1.0; sources.playback_layout.panes.len()]
}

pub(crate) fn generate_thumbnails(sources: PlayerSources, count: u32, channel: Channel) {
  thumbnails::generate(sources, count, channel);
}

pub(crate) fn source_frame_jpeg(
  path: &std::path::Path,
  position_ms: u64,
  duration_ms: u64,
) -> Result<Vec<u8>, String> {
  thumbnails::source_frame_jpeg(path, position_ms, duration_ms)
}
