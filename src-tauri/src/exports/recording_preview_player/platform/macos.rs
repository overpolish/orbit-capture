// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS preview backend: AVFoundation decode, Metal composition.
//!
//! Playback and stills share one `AVAssetReader` pipeline and one GPU
//! compositor, so a paused frame is pixel-identical to the playing frame at
//! that position. Decoded planes stay in Core Video and are presented straight
//! onto the surface's `CAMetalLayer` panes - nothing crosses IPC.

mod composition;
mod cursor;
mod frame;
mod image;
mod scrubber;
mod still;
mod still_decode;
mod thumbnails;
mod video;

use std::{
  process::Child,
  sync::{atomic::AtomicBool, mpsc::SyncSender, Arc, Mutex},
};

use tauri::ipc::Channel;

use super::super::{video::VideoFrame, PlayerSources};

/// macOS decodes paused frames through [`StillDecoder`] rather than through
/// [`spawn_video`].
pub(crate) const NATIVE_STILLS: bool = true;

pub(crate) type StillDecoder = still::NativeStillDecoder;

pub(crate) enum VideoFramePayload {
  Native {
    screen: crate::screenshots::CapturedImage,
    camera: Option<crate::screenshots::CapturedImage>,
    cursor: Option<frame::CursorPreview>,
  },
}

pub(crate) fn send_frame(
  channel: &Channel,
  sources: &PlayerSources,
  request_id: u64,
  payload: VideoFramePayload,
) -> bool {
  match payload {
    VideoFramePayload::Native {
      screen,
      camera,
      cursor,
    } => {
      if let Some(surface) = &sources.preview_surface {
        let screen_presented = surface.present(0, &screen);
        let camera_presented = camera
          .as_ref()
          .is_none_or(|camera| surface.present(1, camera));
        return screen_presented && camera_presented;
      }
      let screen = match composition::encoded_jpeg(&screen) {
        Ok(bytes) => bytes,
        Err(_) => return false,
      };
      let camera = camera.as_ref().map(composition::encoded_jpeg).transpose();
      match camera {
        Ok(camera) => frame::send_frame(
          channel,
          request_id,
          &screen,
          camera.as_deref(),
          cursor.as_ref(),
        ),
        Err(_) => false,
      }
    }
  }
}

/// `still` and `child` are unused here: macOS stills go through
/// [`StillDecoder`], and the decode runs in-process rather than as a child.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_video(
  sources: &PlayerSources,
  playback_factors: &[f64],
  start_ms: u64,
  _still: bool,
  cancelled: Arc<AtomicBool>,
  _child: Arc<Mutex<Option<Child>>>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  video::spawn(sources, playback_factors, start_ms, cancelled, sender)
}

/// How much each pane's playback decode shrinks to match the on-screen pane
/// size, mirroring what the still decoder presents.
pub(crate) fn playback_factors(
  pane_target_sizes: &[(u32, u32)],
  sources: &PlayerSources,
) -> Vec<f64> {
  let composition = sources
    .composition_settings
    .as_ref()
    .and_then(|settings| settings.read().ok().map(|settings| settings.clone()));
  sources
    .playback_layout
    .panes
    .iter()
    .enumerate()
    .map(|(index, pane)| {
      let output_width = composition
        .as_ref()
        .map_or(pane.source_width, |composition| {
          if index == 0 {
            composition.recording_output.primary.width
          } else {
            composition.recording_output.camera.width
          }
        });
      still_decode::pane_factor(pane_target_sizes, index, output_width)
    })
    .collect()
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
