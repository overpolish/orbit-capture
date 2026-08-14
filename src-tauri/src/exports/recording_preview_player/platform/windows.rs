// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows preview backend: Media Foundation hardware decode into D3D11 textures
//! presented by native flip-model swap chains. Live frames never enter system
//! memory or cross the Tauri IPC boundary.

mod decoder;
mod gpu_decoder;
mod still;
mod thumbnails;

use std::{
  process::Child,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender, TrySendError},
    Arc, Mutex,
  },
  time::Duration,
};

use tauri::ipc::Channel;

use self::gpu_decoder::GpuFrame;
pub(crate) use self::gpu_decoder::GpuVideoReader;
use super::super::{video::VideoFrame, PlayerSources};
use crate::exports::preview_platform::ComposedFrame;
use crate::exports::recording_preview_player::video::PREVIEW_FPS;

pub(crate) const NATIVE_STILLS: bool = true;
pub(crate) type StillDecoder = still::NativeStillDecoder;

pub(crate) enum VideoFramePayload {
  Native {
    frame: GpuFrame,
    presented: Option<SyncSender<()>>,
  },
}

pub(crate) fn send_frame(
  _channel: &Channel,
  sources: &PlayerSources,
  _request_id: u64,
  payload: VideoFramePayload,
) -> bool {
  let VideoFramePayload::Native { frame, presented } = payload;
  let result = sources.preview_surface.as_ref().is_some_and(|surface| {
    let settings = sources.composition_settings.as_ref().and_then(|settings| {
      settings
        .read()
        .ok()
        .map(|settings| settings.recording_output.primary.clone())
    });
    settings.is_some_and(|settings| {
      let cursor_settings = sources
        .cursor_settings
        .read()
        .map(|settings| *settings)
        .unwrap_or_default();
      let cursor = sources
        .cursor
        .as_deref()
        .filter(|_| cursor_settings.bake)
        .and_then(|cursor| {
          cursor.gpu_cursor(
            frame.timestamp_ms,
            (frame.width, frame.height),
            cursor_settings,
          )
        });
      surface
        .present_composed_texture(
          0,
          &frame.texture,
          frame.subresource,
          (frame.width, frame.height),
          &settings,
          ComposedFrame {
            cursor,
            seconds: frame.timestamp_ms as f64 / 1_000.0,
          },
        )
        .unwrap_or(false)
    })
  });
  if let Some(presented) = presented {
    let _ = presented.send(());
  }
  result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_video(
  sources: &PlayerSources,
  _playback_factors: &[f64],
  start_ms: u64,
  _still: bool,
  cancelled: Arc<AtomicBool>,
  _child: Arc<Mutex<Option<Child>>>,
  sender: SyncSender<VideoFrame>,
) -> Result<std::thread::JoinHandle<()>, String> {
  if sources.camera_path.is_some() {
    return Err("Windows camera preview is not available yet".to_owned());
  }
  if sources.playback_layout.panes.is_empty() {
    return Err("The recording has no video pane".to_owned());
  }
  let path = sources.screen_path.clone();
  let duration_ms = sources.duration_ms;
  let surface = sources
    .preview_surface
    .clone()
    .ok_or_else(|| "Windows GPU preview has no native presentation surface".to_owned())?;

  let (startup_tx, startup_rx) = mpsc::sync_channel(1);
  let thread = std::thread::Builder::new()
    .name("recording-preview-video-windows".to_owned())
    .spawn(move || {
      let mut reader = match GpuVideoReader::open(&path, start_ms, surface) {
        Ok(reader) => reader,
        Err(error) => {
          let _ = startup_tx.send(Err(error));
          return;
        }
      };
      let _ = startup_tx.send(Ok(()));
      let mut index = 0_u64;
      while !cancelled.load(Ordering::Acquire) {
        let target_ms = start_ms.saturating_add(index * 1_000 / PREVIEW_FPS);
        if target_ms >= duration_ms {
          break;
        }
        let frame = match reader.frame_at(target_ms) {
          Ok(Some(frame)) => frame,
          Ok(None) | Err(_) => break,
        };
        // The decoder's sample owns a pooled DXGI surface. Do not ask Media
        // Foundation for another sample until the consumer has submitted this
        // texture to the compositor, otherwise its pixels can be recycled
        // underneath the older timestamp still waiting in the playback queue.
        let (presented_tx, presented_rx) = mpsc::sync_channel(0);
        let mut frame = VideoFrame {
          timestamp_ms: frame.timestamp_ms,
          payload: VideoFramePayload::Native {
            frame,
            presented: Some(presented_tx),
          },
        };
        loop {
          match sender.try_send(frame) {
            Ok(()) => break,
            Err(TrySendError::Full(returned)) => {
              if cancelled.load(Ordering::Acquire) {
                return;
              }
              frame = returned;
              std::thread::yield_now();
            }
            Err(TrySendError::Disconnected(_)) => return,
          }
        }
        while !cancelled.load(Ordering::Acquire) {
          match presented_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
          }
        }
        index += 1;
      }
    })
    .map_err(|error| error.to_string())?;
  match startup_rx.recv_timeout(Duration::from_secs(5)) {
    Ok(Ok(())) => Ok(thread),
    Ok(Err(error)) => {
      let _ = thread.join();
      Err(error)
    }
    Err(_) => {
      let _ = thread.join();
      Err("Media Foundation did not open the preview in time".to_owned())
    }
  }
}

pub(crate) fn playback_factors(
  pane_target_sizes: &[(u32, u32)],
  sources: &PlayerSources,
) -> Vec<f64> {
  sources
    .playback_layout
    .panes
    .iter()
    .enumerate()
    .map(|(index, pane)| {
      pane_target_sizes
        .get(index)
        .map(|size| f64::from(size.0.max(16)) / f64::from(pane.source_width.max(1)))
        .unwrap_or(0.5)
        .clamp(0.1, 1.0)
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

pub(crate) fn composed_frame_image(
  sources: &PlayerSources,
  position_ms: u64,
  cursor_effects: crate::exports::cursor_effects::CursorEffectSettings,
  recording_output: &crate::exports::RecordingOutputSettings,
) -> Result<crate::screenshots::CapturedImage, String> {
  let surface = sources
    .preview_surface
    .clone()
    .ok_or_else(|| "Windows GPU preview has no native presentation surface".to_owned())?;
  let position_ms = position_ms.min(sources.duration_ms.saturating_sub(1));
  let mut reader = GpuVideoReader::open(&sources.screen_path, position_ms, surface.clone())?;
  let frame = reader
    .frame_at(position_ms)?
    .ok_or_else(|| "Media Foundation returned no source frame".to_owned())?;
  let cursor = sources
    .cursor
    .as_deref()
    .filter(|_| cursor_effects.bake)
    .and_then(|cursor| {
      cursor.gpu_cursor(
        frame.timestamp_ms,
        (frame.width, frame.height),
        cursor_effects,
      )
    });
  surface.compose_texture_to_image(
    &frame.texture,
    frame.subresource,
    (frame.width, frame.height),
    &recording_output.primary,
    ComposedFrame {
      cursor,
      seconds: frame.timestamp_ms as f64 / 1_000.0,
    },
  )
}
