// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  process::Child,
  sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
  },
  time::Duration,
};

use tauri::ipc::Channel;

use super::{audio, send_error, stop_child, PlaybackMode};
use crate::exports::recording_preview_player::{PlayerSources, RecordingPreviewPlayerEvent};
use crate::exports::AudioTrackVolume;

pub(super) struct RunContext {
  pub(super) audio_child: Arc<Mutex<Option<Child>>>,
  pub(super) cancelled: Arc<AtomicBool>,
  pub(super) event_channel: Channel<RecordingPreviewPlayerEvent>,
  pub(super) mode: PlaybackMode,
  pub(super) position_ms: Arc<AtomicU64>,
  pub(super) request_id: u64,
  pub(super) selected_audio: Arc<RwLock<Vec<usize>>>,
  pub(super) audio_volumes: Arc<RwLock<Vec<AudioTrackVolume>>>,
  pub(super) sources: PlayerSources,
  pub(super) start_ms: u64,
}

pub(super) fn run(context: RunContext) {
  let RunContext {
    audio_child,
    cancelled,
    event_channel,
    mode,
    position_ms,
    request_id,
    selected_audio,
    audio_volumes,
    sources,
    start_ms,
  } = context;
  if matches!(mode, PlaybackMode::Still) {
    position_ms.store(start_ms, Ordering::Release);
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Ready {
      position_ms: start_ms,
      request_id,
    });
    return;
  }
  let audio = match audio::spawn(
    &sources,
    selected_audio,
    audio_volumes,
    start_ms,
    Arc::clone(&cancelled),
    Arc::clone(&audio_child),
  ) {
    Ok(audio) => audio,
    Err(error) => return send_error(&event_channel, error),
  };
  let _ = event_channel.send(RecordingPreviewPlayerEvent::Playing {
    position_ms: start_ms,
  });
  while !cancelled.load(Ordering::Acquire) {
    let elapsed =
      audio.played_frames.load(Ordering::Acquire) * 1_000 / u64::from(audio.sample_rate);
    let current = start_ms.saturating_add(elapsed).min(sources.duration_ms);
    position_ms.store(current, Ordering::Release);
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Position {
      position_ms: current,
    });
    if current >= sources.duration_ms {
      break;
    }
    std::thread::sleep(Duration::from_millis(16));
  }
  cancelled.store(true, Ordering::Release);
  stop_child(&audio_child);
  drop(audio.stream);
  let _ = audio.thread.join();
  if position_ms.load(Ordering::Acquire) >= sources.duration_ms.saturating_sub(50) {
    position_ms.store(sources.duration_ms, Ordering::Release);
    let _ = event_channel.send(RecordingPreviewPlayerEvent::Ended);
  }
}
