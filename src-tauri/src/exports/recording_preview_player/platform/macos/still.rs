// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paused-frame and scrub decoding for the native preview player.
//!
//! Stills are decoded by the same `AVAssetReader` pipeline as playback and
//! composed by the same GPU compositor, so a paused frame is pixel-identical
//! to the playing frame at that position. Scrubbing decodes at the presented
//! size and the settled frame is refined at full resolution.

use std::{sync::atomic::Ordering, sync::mpsc, thread::JoinHandle};

use tauri::ipc::Channel;

use super::composition::{cursor_rgba, still_overlay};
use super::cursor::cursor_preview;
use super::image::frame_position;
use super::still_decode::{scaled_output, DecodedFrame, PaneDecoder};
use crate::exports::cursor_effects::CursorOverlayCache;
use crate::exports::recording_preview_player::{PlayerSources, RecordingPreviewPlayerEvent};

enum DecoderCommand {
  Seek {
    position_ms: u64,
    request_id: u64,
    target_sizes: Vec<(u32, u32)>,
    /// A mid-gesture skim: the scrubber may land on the cheapest nearby frame
    /// instead of decoding the exact position.
    rough: bool,
  },
  Stop,
}

pub(crate) struct NativeStillDecoder {
  sender: mpsc::Sender<DecoderCommand>,
  thread: Option<JoinHandle<()>>,
}

struct CachedImages {
  camera: Option<DecodedFrame>,
  camera_ms: Option<u64>,
  screen: DecodedFrame,
  screen_ms: u64,
  sizes: (u32, u32, Option<(u32, u32)>),
}

fn run(
  sources: PlayerSources,
  receiver: mpsc::Receiver<DecoderCommand>,
  event_channel: Channel<RecordingPreviewPlayerEvent>,
) {
  let screen_pane = &sources.playback_layout.panes[0];
  let mut screen = match PaneDecoder::open(
    &sources.screen_path,
    screen_pane.source_width,
    screen_pane.source_height,
    sources.duration_ms,
  ) {
    Ok(decoder) => decoder,
    Err(message) => {
      let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
      return;
    }
  };
  let mut camera = match (
    sources.camera_path.as_deref(),
    sources.camera_duration_ms,
    sources.playback_layout.panes.get(1),
  ) {
    (Some(path), Some(duration_ms), Some(pane)) => {
      match PaneDecoder::open(path, pane.source_width, pane.source_height, duration_ms) {
        Ok(decoder) => Some(decoder),
        Err(message) => {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
          return;
        }
      }
    }
    _ => None,
  };
  let mut cursor_cache = CursorOverlayCache::new();
  let mut image_cache: Option<CachedImages> = None;
  let mut pending_command = None;

  while let Ok(mut command) = pending_command.take().map_or_else(|| receiver.recv(), Ok) {
    while let Ok(next) = receiver.try_recv() {
      command = next;
    }
    let DecoderCommand::Seek {
      position_ms,
      request_id,
      rough,
      target_sizes,
    } = command
    else {
      break;
    };
    // Screen and camera (and any pane rects the layout deferred to this
    // still) reach the screen in one commit, so a canvas resize never shows
    // one pane a display tick ahead of the other. Opened before any early
    // `continue` so a skipped still flushes the deferred layout regardless.
    let _batch = sources
      .preview_surface
      .as_ref()
      .map(|surface| surface.present_batch());
    let composition = sources
      .composition_settings
      .as_ref()
      .and_then(|settings| settings.read().ok().map(|settings| settings.clone()));
    // Paused editing keeps one native-resolution source frame resident. Frame,
    // crop and OSC gestures then only rerun the Metal composition instead of
    // invalidating the decoder cache for every changing pane size. Live
    // playback still uses pane-sized decode factors in `video.rs`.
    let screen_size = screen.decode_size(1.0);
    let camera_size = camera.as_ref().map(|camera| camera.decode_size(1.0));
    let screen_position_ms = frame_position(position_ms, sources.duration_ms);
    let camera_position_ms = camera.as_ref().zip(camera_size).map(|_| {
      frame_position(
        screen_position_ms,
        sources.camera_duration_ms.unwrap_or(sources.duration_ms),
      )
    });
    let sizes_key = (screen_size.0, screen_size.1, camera_size);
    let cache_matches = image_cache.as_ref().is_some_and(|cache| {
      cache.screen_ms == screen_position_ms
        && cache.camera_ms == camera_position_ms
        && cache.sizes == sizes_key
    });
    if !cache_matches {
      let screen_image =
        match screen.frame_at(screen_position_ms, screen_size.0, screen_size.1, rough) {
          Ok(Some(image)) => image,
          Ok(None) => {
            continue;
          }
          Err(message) => {
            let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
            continue;
          }
        };
      let camera_image = match (camera.as_mut(), camera_size, camera_position_ms) {
        (Some(camera), Some((width, height)), Some(camera_ms)) => {
          match camera.frame_at(camera_ms, width, height, rough) {
            Ok(image) => image,
            Err(message) => {
              let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
              continue;
            }
          }
        }
        _ => None,
      };
      image_cache = Some(CachedImages {
        camera: camera_image,
        camera_ms: camera_position_ms,
        screen: screen_image,
        screen_ms: screen_position_ms,
        sizes: sizes_key,
      });
    }
    // A decoded frame is always presented, even when newer seeks are already
    // queued: during a drag newer seeks are the norm, and dropping the frame
    // would starve the screen down to a few updates a second. The loop drains
    // to the newest command right after, so latest-wins still holds.
    let cache = image_cache
      .as_ref()
      .expect("a decoded still is cached after a successful request");
    let cursor_settings = sources
      .cursor_settings
      .read()
      .map(|settings| *settings)
      .unwrap_or_default();
    let cursor = cursor_preview(
      sources.cursor.as_deref(),
      screen_position_ms,
      cursor_settings,
      (
        sources.playback_layout.panes[0].source_width,
        sources.playback_layout.panes[0].source_height,
      ),
      &mut cursor_cache,
    )
    .unwrap_or_default();
    let cursor_pixels = cursor.as_ref().and_then(|cursor| cursor_rgba(cursor).ok());
    // Only live playback may veto a still present; newer queued seeks just
    // mean this frame is a beat old, which still beats a frozen screen.
    if sources.playing.load(Ordering::Acquire) {
      continue;
    }
    let Some(surface) = sources.preview_surface.as_ref() else {
      continue;
    };
    let (screen_presented, camera_presented) = if let Some(composition) = &composition {
      let screen_factor = super::still_decode::pane_factor(
        &target_sizes,
        0,
        composition.recording_output.primary.width,
      );
      let screen_output = scaled_output(&composition.recording_output.primary, screen_factor);
      // Placeholder settings below the compositor's validation floor mean the
      // webview has not sent real output dimensions yet; wait quietly.
      if screen_output.width < 64 || screen_output.height < 64 {
        continue;
      }
      let screen_metadata = cache.screen.metadata();
      let camera_metadata = cache.camera.as_ref().map(DecodedFrame::metadata);
      let (cursor_image, overlay) = match still_overlay(
        &screen_metadata,
        &screen_output,
        cursor.as_ref().zip(cursor_pixels),
        composition
          .bake_camera
          .then_some(camera_metadata.as_ref())
          .flatten(),
        composition
          .bake_camera
          .then_some(composition.camera_overlay),
        composition.recording_output.camera.drop_shadow,
        composition.recording_output.camera_on_top,
      ) {
        Ok(value) => value,
        Err(message) => {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
          continue;
        }
      };
      let baked_camera = composition
        .bake_camera
        .then_some(cache.camera.as_ref())
        .flatten();
      let screen_presented = if let Some(source_pixels) = cache.screen.pixels() {
        surface.present_composed_pixels(
          0,
          screen_position_ms,
          source_pixels,
          cache.screen.dimensions(),
          &screen_output,
          screen_position_ms as f64 / 1_000.0,
          cursor_image.as_ref(),
          baked_camera.and_then(|camera| camera.rgba()),
          baked_camera.and_then(DecodedFrame::pixels),
          overlay.as_ref(),
          cursor_settings.clip_at_video_edge,
        )
      } else if let Some(screen) = cache.screen.rgba() {
        surface.present_composed(
          0,
          screen_position_ms,
          screen,
          &screen_output,
          screen_position_ms as f64 / 1_000.0,
          cursor_image.as_ref(),
          baked_camera.and_then(|camera| camera.rgba()),
          overlay.as_ref(),
          cursor_settings.clip_at_video_edge,
        )
      } else {
        Ok(false)
      }
      .unwrap_or(false);
      let camera_presented = if composition.bake_camera {
        true
      } else {
        cache.camera.as_ref().is_none_or(|camera| {
          // The webview opens with 1x1 placeholder camera output settings
          // until the real dimensions load; presenting would fail dimension
          // validation, so the pane simply waits for the settled settings.
          if composition.recording_output.camera.width < 64
            || composition.recording_output.camera.height < 64
          {
            return true;
          }
          let camera_factor = super::still_decode::pane_factor(
            &target_sizes,
            1,
            composition.recording_output.camera.width,
          );
          let camera_output = scaled_output(&composition.recording_output.camera, camera_factor);
          let presented = if let Some(pixels) = camera.pixels() {
            surface.present_composed_pixels(
              1,
              camera_position_ms.unwrap_or(0),
              pixels,
              camera.dimensions(),
              &camera_output,
              screen_position_ms as f64 / 1_000.0,
              None,
              None,
              None,
              None,
              false,
            )
          } else if let Some(camera) = camera.rgba() {
            surface.present_composed(
              1,
              camera_position_ms.unwrap_or(0),
              camera,
              &camera_output,
              screen_position_ms as f64 / 1_000.0,
              None,
              None,
              None,
              false,
            )
          } else {
            Ok(false)
          };
          presented.unwrap_or(false)
        })
      };
      (screen_presented, camera_presented)
    } else {
      let screen_presented = cache
        .screen
        .rgba()
        .is_some_and(|image| surface.present(0, image));
      let camera_presented = cache
        .camera
        .as_ref()
        .is_none_or(|camera| camera.rgba().is_some_and(|image| surface.present(1, image)));
      (screen_presented, camera_presented)
    };
    if screen_presented && camera_presented {
      let _ = event_channel.send(RecordingPreviewPlayerEvent::Ready {
        position_ms,
        request_id,
      });
    }
    continue;
  }
}

impl NativeStillDecoder {
  pub(crate) fn spawn(
    sources: PlayerSources,
    event_channel: Channel<RecordingPreviewPlayerEvent>,
    _frame_channel: Channel,
  ) -> Result<Self, String> {
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
      .name("recording-preview-still".to_owned())
      .spawn(move || run(sources, receiver, event_channel))
      .map_err(|error| error.to_string())?;
    Ok(Self {
      sender,
      thread: Some(thread),
    })
  }

  pub(crate) fn seek(
    &self,
    position_ms: u64,
    request_id: u64,
    rough: bool,
    target_sizes: Vec<(u32, u32)>,
  ) -> Result<(), String> {
    self
      .sender
      .send(DecoderCommand::Seek {
        position_ms,
        request_id,
        rough,
        target_sizes,
      })
      .map_err(|_| "The native preview decoder stopped".to_owned())
  }

  pub(crate) fn stop(mut self) {
    let _ = self.sender.send(DecoderCommand::Stop);
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}
