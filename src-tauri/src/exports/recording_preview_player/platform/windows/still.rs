// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Long-lived Media Foundation decoder for paused frames and timeline scrubs.

use std::{
  sync::{atomic::Ordering, mpsc},
  thread::JoinHandle,
};

use tauri::ipc::Channel;

use super::gpu_decoder::GpuVideoReader;
use crate::exports::preview_platform::ComposedFrame;
use crate::exports::recording_preview_player::{PlayerSources, RecordingPreviewPlayerEvent};

enum DecoderCommand {
  Seek {
    position_ms: u64,
    request_id: u64,
    rough: bool,
  },
  Stop,
}

pub(crate) struct NativeStillDecoder {
  sender: mpsc::Sender<DecoderCommand>,
  thread: Option<JoinHandle<()>>,
}

impl NativeStillDecoder {
  pub(crate) fn spawn(
    sources: PlayerSources,
    event_channel: Channel<RecordingPreviewPlayerEvent>,
    frame_channel: Channel,
  ) -> Result<Self, String> {
    let (sender, receiver) = mpsc::channel();
    let thread = std::thread::Builder::new()
      .name("recording-preview-still-windows".to_owned())
      .spawn(move || run(sources, receiver, event_channel, frame_channel))
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
    _target_sizes: Vec<(u32, u32)>,
  ) -> Result<(), String> {
    self
      .sender
      .send(DecoderCommand::Seek {
        position_ms,
        request_id,
        rough,
      })
      .map_err(|_| "The Windows preview decoder is no longer running".to_owned())
  }

  pub(crate) fn stop(mut self) {
    let _ = self.sender.send(DecoderCommand::Stop);
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

fn run(
  sources: PlayerSources,
  receiver: mpsc::Receiver<DecoderCommand>,
  event_channel: Channel<RecordingPreviewPlayerEvent>,
  _frame_channel: Channel,
) {
  if sources.camera_path.is_some() {
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Error {
      message: "Windows camera preview is not available yet".to_owned(),
    });
    return;
  }
  let Some(surface) = sources.preview_surface.clone() else {
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Error {
      message: "Windows GPU preview has no native presentation surface".to_owned(),
    });
    return;
  };
  let mut reader: Option<GpuVideoReader> = None;
  let mut pending = None;
  while let Ok(mut command) = pending.take().map_or_else(|| receiver.recv(), Ok) {
    while let Ok(next) = receiver.try_recv() {
      command = next;
    }
    let DecoderCommand::Seek {
      position_ms,
      request_id,
      rough,
    } = command
    else {
      break;
    };
    if reader.is_none() {
      match GpuVideoReader::open(&sources.screen_path, position_ms, surface.clone()) {
        Ok(opened) => {
          reader = Some(opened);
        }
        Err(message) => {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
          continue;
        }
      }
    } else if let Some(reader) = reader.as_mut() {
      let current = reader.last_timestamp_ms();
      if should_seek(current, position_ms, rough) {
        if let Err(message) = reader.seek(position_ms) {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
          continue;
        }
      }
    }
    let Some(reader) = reader.as_mut() else {
      continue;
    };
    let frame = match reader.frame_at(position_ms) {
      Ok(Some(frame)) => frame,
      Ok(None) => continue,
      Err(message) => {
        let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
        continue;
      }
    };
    if sources.playing.load(Ordering::Acquire) {
      continue;
    }
    let settings = sources.composition_settings.as_ref().and_then(|settings| {
      settings
        .read()
        .ok()
        .map(|settings| settings.recording_output.primary.clone())
    });
    if settings.is_some_and(|settings| {
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
    }) {
      let _ = event_channel.send(RecordingPreviewPlayerEvent::Ready {
        position_ms,
        request_id,
      });
    }
  }
}

// Small forward moves are cheaper to satisfy from the decoder's current GPU
// stream. Larger jumps seek directly so rapid timeline scrubs cannot leave a
// growing queue of intermediate frames to decode. The final (non-rough) seek
// always resets to the exact requested position.
const MAX_SEQUENTIAL_SCRUB_MS: u64 = 250;

fn should_seek(current_ms: u64, position_ms: u64, rough: bool) -> bool {
  !rough
    || position_ms < current_ms
    || position_ms.saturating_sub(current_ms) > MAX_SEQUENTIAL_SCRUB_MS
}

#[cfg(test)]
mod tests {
  use super::should_seek;

  #[test]
  fn rough_scrubs_only_decode_short_forward_steps_sequentially() {
    assert!(!should_seek(4_000, 4_200, true));
    assert!(should_seek(4_000, 4_251, true));
    assert!(should_seek(4_000, 3_999, true));
  }

  #[test]
  fn settled_scrubs_always_seek_exactly() {
    assert!(should_seek(4_000, 4_001, false));
    assert!(should_seek(4_000, 4_000, false));
  }
}
