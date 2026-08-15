// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Long-lived Media Foundation decoder for paused frames and timeline scrubs.

use std::{
  sync::{atomic::Ordering, mpsc},
  thread::JoinHandle,
};

use tauri::ipc::Channel;

use super::{gpu_decoder::GpuVideoReader, present_native_frame};
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
  let Some(surface) = sources.preview_surface.clone() else {
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Error {
      message: "Windows GPU preview has no native presentation surface".to_owned(),
    });
    return;
  };
  let mut paths = vec![sources.screen_path.clone()];
  if let Some(path) = &sources.camera_path {
    paths.push(path.clone());
  }
  let mut readers = (0..paths.len()).map(|_| None).collect::<Vec<_>>();
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
    let mut presented = true;
    for (index, (path, reader)) in paths.iter().zip(readers.iter_mut()).enumerate() {
      // Repeated SetCurrentPosition calls can leave Media Foundation's D3D
      // source reader at a premature EOF even though later samples exist.
      // A settled seek must be authoritative, so give it a fresh native GPU
      // reader. Rough seeks keep their warm reader unless it demonstrably
      // falls behind, preserving fast drag feedback.
      if reader.is_none() || !rough {
        match GpuVideoReader::open(path, position_ms, surface.clone()) {
          Ok(opened) => *reader = Some(opened),
          Err(message) => {
            let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
            presented = false;
            break;
          }
        }
      } else if let Some(reader) = reader.as_mut() {
        if should_seek(reader.last_timestamp_ms(), position_ms, rough) {
          if let Err(message) = reader.seek(position_ms) {
            let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
            presented = false;
            break;
          }
        }
      }
      let decoded = match reader
        .as_mut()
        .and_then(|reader| reader.frame_at(position_ms).transpose())
      {
        Some(Ok(frame)) => Some(frame),
        Some(Err(message)) => {
          let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
          presented = false;
          break;
        }
        None => None,
      };
      let needs_reopen = decoded
        .as_ref()
        .is_none_or(|frame| frame_is_stale(frame.timestamp_ms, position_ms));
      let frame = if needs_reopen {
        match GpuVideoReader::open(path, position_ms, surface.clone()).and_then(|mut opened| {
          let frame = opened.frame_at(position_ms)?;
          *reader = Some(opened);
          frame.ok_or_else(|| "Media Foundation returned no frame after reopening".to_owned())
        }) {
          Ok(recovered) => recovered,
          Err(message) => {
            let _ = event_channel.send(RecordingPreviewPlayerEvent::Error { message });
            presented = false;
            break;
          }
        }
      } else {
        decoded.expect("a non-stale decoded frame exists")
      };
      if sources.playing.load(Ordering::Acquire) {
        presented = false;
        break;
      }
      presented &= present_native_frame(&sources, index as u32, &frame);
    }
    if presented {
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
const MAX_STILL_LAG_MS: u64 = 100;

fn frame_is_stale(frame_ms: u64, position_ms: u64) -> bool {
  frame_ms.saturating_add(MAX_STILL_LAG_MS) < position_ms
}

fn should_seek(current_ms: u64, position_ms: u64, rough: bool) -> bool {
  !rough
    || position_ms < current_ms
    || position_ms.saturating_sub(current_ms) > MAX_SEQUENTIAL_SCRUB_MS
}

#[cfg(test)]
mod tests {
  use super::{frame_is_stale, should_seek};

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

  #[test]
  fn a_gpu_reader_that_hits_early_eof_is_reopened() {
    assert!(frame_is_stale(17_891, 19_480));
    assert!(!frame_is_stale(19_430, 19_480));
  }
}
