// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lightweight confidence signals from the active capture session.
//!
//! The dock subscribes to this shared boundary; it never opens a second
//! microphone, camera or system-audio session. Platform capture adapters only
//! need to feed levels and occasional small camera frames into this type.

use std::{sync::Mutex, time::Instant};

use tauri::ipc::{Channel, InvokeResponseBody};

const SOURCES_EVENT: u8 = 0;
const SYSTEM_AUDIO_EVENT: u8 = 1;
const MICROPHONE_EVENT: u8 = 2;
const CAMERA_EVENT: u8 = 3;
const SYSTEM_AUDIO_FLAG: u8 = 1;
const MICROPHONE_FLAG: u8 = 2;
const CAMERA_FLAG: u8 = 4;
const LEVEL_INTERVAL_SECONDS: f32 = 1.0 / 30.0;

#[derive(Default)]
struct LevelSignal {
  last_sent: Option<Instant>,
  peak: f32,
}

impl LevelSignal {
  fn push(&mut self, samples: &[f32], now: Instant) -> Option<f32> {
    self.peak = samples
      .iter()
      .copied()
      .map(f32::abs)
      .fold(self.peak, f32::max);
    if self
      .last_sent
      .is_some_and(|last| now.duration_since(last).as_secs_f32() < LEVEL_INTERVAL_SECONDS)
    {
      return None;
    }
    let decibels = 20.0 * self.peak.max(1e-8).log10();
    self.peak = 0.0;
    self.last_sent = Some(now);
    Some(decibels)
  }
}

#[derive(Clone)]
struct Subscription {
  channel: Channel,
  id: u64,
}

#[derive(Default)]
struct MonitorState {
  camera: bool,
  microphone_level: LevelSignal,
  microphone: bool,
  subscription: Option<Subscription>,
  system_audio: bool,
  system_audio_level: LevelSignal,
}

#[derive(Default)]
pub(crate) struct RecordingMonitor(Mutex<MonitorState>);

impl RecordingMonitor {
  pub(crate) fn configure(&self, system_audio: bool, microphone: bool, camera: bool) {
    let channel = {
      let mut state = self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      state.system_audio = system_audio;
      state.microphone = microphone;
      state.camera = camera;
      state.microphone_level = LevelSignal::default();
      state.system_audio_level = LevelSignal::default();
      state
        .subscription
        .as_ref()
        .map(|value| value.channel.clone())
    };
    if let Some(channel) = channel {
      send_sources(&channel, system_audio, microphone, camera);
    }
  }

  pub(crate) fn subscribe(&self, id: u64, channel: Channel) {
    let (system_audio, microphone, camera) = {
      let mut state = self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      if state
        .subscription
        .as_ref()
        .is_some_and(|current| current.id > id)
      {
        return;
      }
      state.subscription = Some(Subscription {
        channel: channel.clone(),
        id,
      });
      (state.system_audio, state.microphone, state.camera)
    };
    send_sources(&channel, system_audio, microphone, camera);
  }

  pub(crate) fn unsubscribe(&self, id: u64) {
    let mut state = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state
      .subscription
      .as_ref()
      .is_some_and(|value| value.id == id)
    {
      state.subscription = None;
    }
  }

  pub(crate) fn is_subscribed(&self) -> bool {
    self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .subscription
      .is_some()
  }

  pub(crate) fn send_system_audio(&self, samples: &[f32]) {
    self.send_level(SYSTEM_AUDIO_EVENT, samples);
  }

  pub(crate) fn send_microphone(&self, samples: &[f32]) {
    self.send_level(MICROPHONE_EVENT, samples);
  }

  pub(crate) fn send_camera(&self, width: u16, height: u16, rgba: Vec<u8>) {
    let mut payload = Vec::with_capacity(5 + rgba.len());
    payload.push(CAMERA_EVENT);
    payload.extend_from_slice(&width.to_le_bytes());
    payload.extend_from_slice(&height.to_le_bytes());
    payload.extend(rgba);
    self.send(payload);
  }

  fn send_level(&self, event: u8, samples: &[f32]) {
    let (channel, decibels) = {
      let mut state = self
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
      let decibels = match event {
        SYSTEM_AUDIO_EVENT => state.system_audio_level.push(samples, Instant::now()),
        MICROPHONE_EVENT => state.microphone_level.push(samples, Instant::now()),
        _ => None,
      };
      (
        state
          .subscription
          .as_ref()
          .map(|value| value.channel.clone()),
        decibels,
      )
    };
    let (Some(channel), Some(decibels)) = (channel, decibels) else {
      return;
    };
    let mut payload = Vec::with_capacity(5);
    payload.push(event);
    payload.extend_from_slice(&decibels.to_le_bytes());
    let _ = channel.send(InvokeResponseBody::Raw(payload));
  }

  fn send(&self, payload: Vec<u8>) {
    let channel = self
      .0
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .subscription
      .as_ref()
      .map(|value| value.channel.clone());
    if let Some(channel) = channel {
      let _ = channel.send(InvokeResponseBody::Raw(payload));
    }
  }
}

fn send_sources(channel: &Channel, system_audio: bool, microphone: bool, camera: bool) {
  let flags = (u8::from(system_audio) * SYSTEM_AUDIO_FLAG)
    | (u8::from(microphone) * MICROPHONE_FLAG)
    | (u8::from(camera) * CAMERA_FLAG);
  let _ = channel.send(InvokeResponseBody::Raw(vec![SOURCES_EVENT, flags]));
}
