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
use crate::{
  exports::{
    cursor_effects::{CursorEffectSettings, CursorOverlayCache},
    CameraOverlaySettings, RecordingOutputSettings,
  },
  screenshots::{compose_output_layers, CapturedImage},
};

/// macOS decodes paused frames through [`StillDecoder`] rather than through
/// [`spawn_video`].
pub(crate) const NATIVE_STILLS: bool = true;

pub(crate) type StillDecoder = still::NativeStillDecoder;

pub(crate) enum VideoFramePayload {
  Native {
    screen: crate::screenshots::CapturedImage,
    camera: Option<crate::screenshots::CapturedImage>,
    cursor: Option<frame::CursorPreview>,
    screen_output: crate::screenshots::ScreenshotOutputSettings,
    camera_output: crate::screenshots::ScreenshotOutputSettings,
    cursor_image: Option<crate::screenshots::CapturedImage>,
    overlay: Option<crate::screenshots::StillOverlay>,
    bake_camera: bool,
    seconds: f64,
    clip_cursor_at_video_edge: bool,
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
      screen_output,
      camera_output,
      cursor_image,
      overlay,
      bake_camera,
      seconds,
      clip_cursor_at_video_edge,
    } => {
      if let Some(surface) = &sources.preview_surface {
        use crate::exports::preview_platform::{NativeWorkspacePlacement, RecordingWorkspaceLayer};
        let source_token = (seconds * 1_000.0).round().max(0.0) as u64;
        let mut layers = vec![RecordingWorkspaceLayer {
          pane_index: 0,
          source_token: source_token << 2,
          source: Some(&screen),
          source_pixels: None,
          settings: screen_output,
          placement: NativeWorkspacePlacement::default(),
          seconds,
          cursor: cursor_image.as_ref(),
          camera: bake_camera.then_some(camera.as_ref()).flatten(),
          camera_pixels: None,
          overlay: overlay.as_ref(),
          clip_cursor_at_video_edge,
          foreground_only: false,
        }];
        if !bake_camera {
          if let Some(camera) = camera.as_ref() {
            layers.push(RecordingWorkspaceLayer {
              pane_index: 1,
              source_token: (source_token << 2) | 1,
              source: Some(camera),
              source_pixels: None,
              settings: camera_output,
              placement: NativeWorkspacePlacement::default(),
              seconds,
              cursor: None,
              camera: None,
              camera_pixels: None,
              overlay: None,
              clip_cursor_at_video_edge: false,
              foreground_only: false,
            });
          }
        }
        return surface
          .present_recording_workspace(&layers)
          .unwrap_or(false);
      }
      let screen = match crate::screenshots::compose_output_layers(
        &screen,
        &screen_output,
        seconds,
        true,
        cursor_image.as_ref(),
        bake_camera.then_some(camera.as_ref()).flatten(),
        overlay.as_ref(),
        clip_cursor_at_video_edge,
        false,
      ) {
        Ok(screen) => screen,
        Err(_) => return false,
      };
      let camera = if bake_camera {
        None
      } else {
        camera.as_ref().and_then(|camera| {
          crate::screenshots::compose_output_layers(
            camera,
            &camera_output,
            seconds,
            true,
            None,
            None,
            None,
            false,
            false,
          )
          .ok()
        })
      };
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

/// Renders one clipboard frame through the same Metal still compositor as the
/// live preview, including the baked camera and cursor layer ordering.
pub(crate) fn composed_frame_image(
  sources: &PlayerSources,
  position_ms: u64,
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  cursor_effects: CursorEffectSettings,
  recording_output: &RecordingOutputSettings,
) -> Result<CapturedImage, String> {
  let position_ms = position_ms.min(sources.duration_ms.saturating_sub(1));
  let screen = composition::decoded_rgba(&source_frame_jpeg(
    &sources.screen_path,
    position_ms,
    sources.duration_ms,
  )?)?;
  let camera = if bake_camera {
    sources
      .camera_path
      .as_ref()
      .map(|path| {
        let duration_ms = sources.camera_duration_ms.unwrap_or(sources.duration_ms);
        composition::decoded_rgba(&source_frame_jpeg(path, position_ms, duration_ms)?)
      })
      .transpose()?
  } else {
    None
  };
  let mut cursor_cache = CursorOverlayCache::new();
  let cursor = cursor::cursor_preview(
    sources.cursor.as_deref(),
    position_ms,
    cursor_effects,
    (screen.width, screen.height),
    &mut cursor_cache,
  )?;
  let cursor_image = cursor.as_ref().map(composition::cursor_rgba).transpose()?;
  let (cursor_image, overlay) = composition::still_overlay(
    &screen,
    &recording_output.primary,
    cursor.as_ref().zip(cursor_image),
    camera.as_ref(),
    camera.as_ref().map(|_| camera_overlay),
    recording_output.camera.drop_shadow,
    recording_output.camera_on_top,
  )?;
  compose_output_layers(
    &screen,
    &recording_output.primary,
    position_ms as f64 / 1_000.0,
    true,
    cursor_image.as_ref(),
    camera.as_ref(),
    overlay.as_ref(),
    cursor_effects.clip_at_video_edge,
    false,
  )
}
