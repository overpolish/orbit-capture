// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
  collections::VecDeque,
  io::Read,
  process::{Child, Command, Stdio},
  sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
  },
  time::Duration,
};

use cpal::{
  traits::{DeviceTrait, HostTrait, StreamTrait},
  FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};

use super::PlayerSources;
use crate::exports::media_preview;

const MAX_QUEUED_SECONDS: usize = 2;
const PREBUFFER_MILLISECONDS: usize = 120;

pub(super) struct AudioPlayback {
  pub played_frames: Arc<AtomicU64>,
  pub sample_rate: u32,
  pub stream: Stream,
  pub thread: std::thread::JoinHandle<()>,
}

fn build_output<T>(
  device: &cpal::Device,
  config: &StreamConfig,
  queue: Arc<Mutex<VecDeque<f32>>>,
  played_frames: Arc<AtomicU64>,
  selected_audio: Arc<RwLock<Vec<usize>>>,
  stream_indices: Vec<usize>,
) -> Result<Stream, String>
where
  T: SizedSample + FromSample<f32>,
{
  let output_channels = usize::from(config.channels);
  let track_count = stream_indices.len();
  device
    .build_output_stream(
      *config,
      move |output: &mut [T], _| {
        let mut queue = queue.lock().unwrap_or_else(|value| value.into_inner());
        let selected = selected_audio
          .read()
          .unwrap_or_else(|value| value.into_inner());
        for frame in output.chunks_mut(output_channels) {
          let mut mixed = 0.0_f32;
          for stream_index in stream_indices.iter().take(track_count) {
            let sample = queue.pop_front().unwrap_or(0.0);
            if selected.contains(stream_index) {
              mixed += sample;
            }
          }
          let mixed = mixed.clamp(-1.0, 1.0);
          for sample in frame {
            *sample = T::from_sample(mixed);
          }
          played_frames.fetch_add(1, Ordering::Relaxed);
        }
      },
      |_| {},
      None,
    )
    .map_err(|error| error.to_string())
}

fn output_stream(
  queue: Arc<Mutex<VecDeque<f32>>>,
  selected_audio: Arc<RwLock<Vec<usize>>>,
  stream_indices: Vec<usize>,
) -> Result<(Stream, Arc<AtomicU64>, StreamConfig), String> {
  let device = cpal::default_host()
    .default_output_device()
    .ok_or_else(|| "No audio output device is available".to_owned())?;
  let supported = device
    .default_output_config()
    .map_err(|error| error.to_string())?;
  let config: StreamConfig = supported.config();
  let played = Arc::new(AtomicU64::new(0));
  let build = |format| match format {
    SampleFormat::F32 => build_output::<f32>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::F64 => build_output::<f64>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::I8 => build_output::<i8>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::I16 => build_output::<i16>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::I24 => build_output::<cpal::I24>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::I32 => build_output::<i32>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::I64 => build_output::<i64>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::U8 => build_output::<u8>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::U16 => build_output::<u16>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::U24 => build_output::<cpal::U24>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::U32 => build_output::<u32>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    SampleFormat::U64 => build_output::<u64>(
      &device,
      &config,
      Arc::clone(&queue),
      Arc::clone(&played),
      Arc::clone(&selected_audio),
      stream_indices.clone(),
    ),
    format => Err(format!("Unsupported audio output format: {format}")),
  };
  let stream = build(supported.sample_format())?;
  Ok((stream, played, config))
}

fn args(sources: &PlayerSources, start_ms: u64, config: &StreamConfig) -> Vec<String> {
  let stream_indices = sources
    .audio_tracks
    .iter()
    .map(|track| track.stream_index)
    .collect::<Vec<_>>();
  let mut args = vec![
    "-hide_banner".to_owned(),
    "-loglevel".to_owned(),
    "error".to_owned(),
    "-nostdin".to_owned(),
    "-ss".to_owned(),
    format!("{}.{:03}", start_ms / 1_000, start_ms % 1_000),
    "-i".to_owned(),
    sources.screen_path.to_string_lossy().into_owned(),
  ];
  let remaining_ms = sources.duration_ms.saturating_sub(start_ms).max(1);
  let remaining = format!("{}.{:03}", remaining_ms / 1_000, remaining_ms % 1_000);
  if stream_indices.len() == 1 {
    args.extend(["-map".to_owned(), format!("0:a:{}", stream_indices[0])]);
  } else {
    let mut filter = String::new();
    let mut inputs = String::new();
    for (position, stream_index) in stream_indices.iter().enumerate() {
      filter.push_str(&format!(
        "[0:a:{stream_index}]aresample={},aformat=sample_fmts=flt:channel_layouts=mono,apad=whole_dur={remaining}[track{position}];",
        config.sample_rate,
      ));
      inputs.push_str(&format!("[track{position}]"));
    }
    filter.push_str(&format!(
      "{inputs}amerge=inputs={}[tracks]",
      stream_indices.len()
    ));
    args.extend([
      "-filter_complex".to_owned(),
      filter,
      "-map".to_owned(),
      "[tracks]".to_owned(),
    ]);
  }
  args.extend([
    "-vn".to_owned(),
    "-ac".to_owned(),
    stream_indices.len().to_string(),
    "-ar".to_owned(),
    config.sample_rate.to_string(),
    "-t".to_owned(),
    remaining,
    "-f".to_owned(),
    "f32le".to_owned(),
    "pipe:1".to_owned(),
  ]);
  args
}

pub(super) fn spawn(
  sources: &PlayerSources,
  selected_audio: Arc<RwLock<Vec<usize>>>,
  start_ms: u64,
  cancelled: Arc<AtomicBool>,
  child: Arc<Mutex<Option<Child>>>,
) -> Result<AudioPlayback, String> {
  let queue = Arc::new(Mutex::new(VecDeque::new()));
  let stream_indices = sources
    .audio_tracks
    .iter()
    .map(|track| track.stream_index)
    .collect::<Vec<_>>();
  let track_count = stream_indices.len();
  let (stream, played_frames, config) = output_stream(
    Arc::clone(&queue),
    Arc::clone(&selected_audio),
    stream_indices,
  )?;
  let mut process = Command::new(media_preview::ffmpeg_path());
  process
    .args(args(sources, start_ms, &config))
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
  let mut process = process
    .spawn()
    .map_err(|error| format!("FFmpeg could not start preview audio: {error}"))?;
  let mut stdout = process
    .stdout
    .take()
    .ok_or_else(|| "FFmpeg did not expose preview audio".to_owned())?;
  *child
    .lock()
    .map_err(|_| "The preview audio process is unavailable".to_owned())? = Some(process);
  let maximum = config.sample_rate as usize * track_count * MAX_QUEUED_SECONDS;
  let thread_queue = Arc::clone(&queue);
  let thread_cancelled = Arc::clone(&cancelled);
  let thread = std::thread::Builder::new()
    .name("recording-preview-audio".to_owned())
    .spawn(move || {
      let mut bytes = vec![0_u8; 16 * 1_024];
      while !thread_cancelled.load(Ordering::Acquire) {
        let count = match stdout.read(&mut bytes) {
          Ok(0) | Err(_) => break,
          Ok(count) => count - count % 4,
        };
        let mut queue = thread_queue
          .lock()
          .unwrap_or_else(|value| value.into_inner());
        for chunk in bytes[..count].chunks_exact(4) {
          queue.push_back(f32::from_le_bytes(chunk.try_into().unwrap_or([0; 4])));
        }
        drop(queue);
        while thread_queue
          .lock()
          .unwrap_or_else(|value| value.into_inner())
          .len()
          > maximum
          && !thread_cancelled.load(Ordering::Acquire)
        {
          std::thread::sleep(Duration::from_millis(5));
        }
      }
    })
    .map_err(|error| error.to_string())?;

  let minimum = config.sample_rate as usize * track_count * PREBUFFER_MILLISECONDS / 1_000;
  while queue
    .lock()
    .unwrap_or_else(|value| value.into_inner())
    .len()
    < minimum
    && !cancelled.load(Ordering::Acquire)
    && !thread.is_finished()
  {
    std::thread::sleep(Duration::from_millis(5));
  }
  stream.play().map_err(|error| error.to_string())?;
  Ok(AudioPlayback {
    played_frames,
    sample_rate: config.sample_rate,
    stream,
    thread,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::exports::recording_preview_player::layout::PreviewPaneKind;
  use crate::exports::{AudioTrackKind, RecordingAudioTrack};

  #[test]
  fn decodes_tracks_into_independent_pcm_channels() {
    let layout = super::super::preview_layout(
      Some((1_920, 1_080, PreviewPaneKind::Screen)),
      None,
      super::super::PREVIEW_HEIGHT,
    );
    let sources = PlayerSources {
      audio_tracks: vec![
        RecordingAudioTrack {
          kind: AudioTrackKind::SystemAudio,
          label: "System audio".to_owned(),
          stream_index: 0,
        },
        RecordingAudioTrack {
          kind: AudioTrackKind::Microphone,
          label: "Microphone".to_owned(),
          stream_index: 1,
        },
      ],
      camera_duration_ms: None,
      camera_path: None,
      duration_ms: 1_000,
      layout: layout.clone(),
      playback_layout: layout,
      screen_path: "/tmp/recording.mov".into(),
    };
    let config = StreamConfig {
      channels: 2,
      sample_rate: 48_000,
      buffer_size: cpal::BufferSize::Default,
    };
    let rendered = args(&sources, 250, &config).join(" ");

    assert!(rendered.contains("[0:a:0]aresample=48000"));
    assert!(rendered.contains("[0:a:1]aresample=48000"));
    assert!(rendered.contains("apad=whole_dur=0.750"));
    assert!(rendered.contains("amerge=inputs=2[tracks]"));
    assert!(rendered.contains("-ac 2"));
  }
}
