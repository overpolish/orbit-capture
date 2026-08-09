// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  collections::VecDeque,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
  },
  thread::{self, JoinHandle},
  time::Duration,
};

use tauri::ipc::Channel;
use wasapi::{
  initialize_mta, AudioCaptureClient, AudioClient, Direction, Handle, SampleType, StreamMode,
  WaveFormat,
};

use super::{AudioPreviewEvent, LevelAccumulator};

pub struct ProcessLoopbackPreview {
  stop: Arc<AtomicBool>,
  threads: Vec<JoinHandle<()>>,
}

impl Drop for ProcessLoopbackPreview {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::Release);
    for thread in self.threads.drain(..) {
      let _ = thread.join();
    }
  }
}

pub fn start_process_loopback_preview(
  mut process_ids: Vec<u32>,
  channel: Channel<AudioPreviewEvent>,
) -> Result<ProcessLoopbackPreview, String> {
  process_ids.sort_unstable();
  process_ids.dedup();
  if process_ids.is_empty() {
    return Err("The selected application has no running processes".into());
  }

  let stop = Arc::new(AtomicBool::new(false));
  let mut preview = ProcessLoopbackPreview {
    stop: Arc::clone(&stop),
    threads: Vec::with_capacity(process_ids.len()),
  };

  for process_id in process_ids {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread_stop = Arc::clone(&stop);
    let thread_channel = channel.clone();
    preview.threads.push(thread::spawn(move || {
      capture_process(process_id, thread_stop, thread_channel, ready_tx);
    }));

    ready_rx
      .recv_timeout(Duration::from_secs(10))
      .map_err(|_| format!("Timed out starting audio capture for process {process_id}"))??;
  }

  Ok(preview)
}

fn capture_process(
  process_id: u32,
  stop: Arc<AtomicBool>,
  channel: Channel<AudioPreviewEvent>,
  ready: mpsc::SyncSender<Result<(), String>>,
) {
  if initialize_mta().is_err() {
    let _ = ready.send(Err("Could not initialize Windows audio capture".into()));
    return;
  }

  let capture = initialize_capture(process_id);
  let (audio_client, capture_client, event) = match capture {
    Ok(capture) => {
      let _ = ready.send(Ok(()));
      capture
    }
    Err(error) => {
      let _ = ready.send(Err(error));
      wasapi::deinitialize();
      return;
    }
  };

  let result = capture_levels(&capture_client, &event, &stop, &channel);
  if let Err(message) = result {
    let _ = channel.send(AudioPreviewEvent::Error { message });
  }

  let _ = audio_client.stop_stream();
  drop(event);
  drop(capture_client);
  drop(audio_client);
  wasapi::deinitialize();
}

fn initialize_capture(
  process_id: u32,
) -> Result<(AudioClient, AudioCaptureClient, Handle), String> {
  let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
  let mut audio_client = AudioClient::new_application_loopback_client(process_id, true)
    .map_err(|error| error.to_string())?;
  audio_client
    .initialize_client(
      &format,
      &Direction::Capture,
      &StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
      },
    )
    .map_err(|error| error.to_string())?;
  let event = audio_client
    .set_get_eventhandle()
    .map_err(|error| error.to_string())?;
  let capture_client = audio_client
    .get_audiocaptureclient()
    .map_err(|error| error.to_string())?;
  audio_client
    .start_stream()
    .map_err(|error| error.to_string())?;
  Ok((audio_client, capture_client, event))
}

fn capture_levels(
  capture_client: &AudioCaptureClient,
  event: &Handle,
  stop: &AtomicBool,
  channel: &Channel<AudioPreviewEvent>,
) -> Result<(), String> {
  let mut bytes = VecDeque::new();
  let mut level = LevelAccumulator::with_format(48_000, 2);

  while !stop.load(Ordering::Acquire) {
    while capture_client
      .get_next_packet_size()
      .map_err(|error| error.to_string())?
      .is_some_and(|frames| frames > 0)
    {
      capture_client
        .read_from_device_to_deque(&mut bytes)
        .map_err(|error| error.to_string())?;
    }

    let sample_count = bytes.len() / size_of::<f32>();
    level.push(
      (0..sample_count).map(|_| {
        f64::from(f32::from_ne_bytes([
          bytes.pop_front().unwrap_or_default(),
          bytes.pop_front().unwrap_or_default(),
          bytes.pop_front().unwrap_or_default(),
          bytes.pop_front().unwrap_or_default(),
        ]))
      }),
      |decibels| {
        let _ = channel.send(AudioPreviewEvent::Signal { decibels });
      },
    );

    let _ = event.wait_for_event(50);
  }

  Ok(())
}
