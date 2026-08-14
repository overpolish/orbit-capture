// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows recording audio.
//!
//! Media Foundation's MP4 sink accepts only one audio stream. Microphone and
//! system audio are therefore captured as timestamped float PCM sidecars and
//! muxed into independent AAC streams after the video sink is finalized. The
//! H.264 video is stream-copied, so this adds no video decode or CPU render.

use std::{
  fs::{self, File},
  io::{BufWriter, Write},
  path::{Path, PathBuf},
  process::Command,
  sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, OnceLock,
  },
  thread::{self, JoinHandle},
  time::{Duration, Instant},
};

use cpal::Stream;
use wasapi::{
  initialize_mta, AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, Handle, SampleType,
  StreamMode, WaveFormat,
};

use crate::recording::{
  encoding::FailureReport, microphone, monitor::RecordingMonitor, SystemAudioSelection,
};

const SYSTEM_SAMPLE_RATE: u32 = 48_000;
const SYSTEM_CHANNELS: u16 = 2;

struct Packet {
  captured_at: Instant,
  samples: Vec<f32>,
}

#[derive(Clone)]
struct RawSource {
  channels: u16,
  first_offset_ms: u64,
  path: PathBuf,
  sample_rate: u32,
}

struct RawSink {
  cleanup_on_drop: bool,
  path: PathBuf,
  sender: Option<mpsc::SyncSender<Packet>>,
  worker: Option<JoinHandle<Result<RawSource, String>>>,
}

impl RawSink {
  fn start(
    path: PathBuf,
    sample_rate: u32,
    channels: u16,
    origin: Arc<OnceLock<Instant>>,
  ) -> Result<Self, String> {
    let (sender, packets) = mpsc::sync_channel(128);
    let worker_path = path.clone();
    let worker = thread::Builder::new()
      .name("orbit-windows-audio-writer".to_owned())
      .spawn(move || write_raw(worker_path, sample_rate, channels, origin, packets))
      .map_err(|error| error.to_string())?;
    Ok(Self {
      cleanup_on_drop: true,
      path,
      sender: Some(sender),
      worker: Some(worker),
    })
  }

  fn sender(&self) -> Result<mpsc::SyncSender<Packet>, String> {
    self
      .sender
      .as_ref()
      .cloned()
      .ok_or_else(|| "The audio capture has already stopped".to_owned())
  }

  fn finish(mut self) -> Result<RawSource, String> {
    self.sender.take();
    let result = self
      .worker
      .take()
      .ok_or_else(|| "The audio writer is unavailable".to_owned())?
      .join()
      .map_err(|_| "The audio writer stopped unexpectedly".to_owned())?;
    self.cleanup_on_drop = false;
    result
  }
}

impl Drop for RawSink {
  fn drop(&mut self) {
    self.sender.take();
    if let Some(worker) = self.worker.take() {
      let _ = worker.join();
    }
    if self.cleanup_on_drop {
      let _ = fs::remove_file(&self.path);
    }
  }
}

fn write_raw(
  path: PathBuf,
  sample_rate: u32,
  channels: u16,
  origin: Arc<OnceLock<Instant>>,
  packets: mpsc::Receiver<Packet>,
) -> Result<RawSource, String> {
  let file = File::create(&path).map_err(|error| error.to_string())?;
  let mut file = BufWriter::new(file);
  let channels_usize = usize::from(channels.max(1));
  let mut first_at = None;
  for mut packet in packets {
    let Some(origin) = origin.get().copied() else {
      continue;
    };
    let frames = packet.samples.len() / channels_usize;
    if frames == 0 {
      continue;
    }
    let packet_duration = Duration::from_secs_f64(frames as f64 / f64::from(sample_rate));
    let packet_end = packet
      .captured_at
      .checked_add(packet_duration)
      .unwrap_or(packet.captured_at);
    if packet_end <= origin {
      continue;
    }
    if packet.captured_at < origin {
      let skip_frames = (origin.duration_since(packet.captured_at).as_secs_f64()
        * f64::from(sample_rate))
      .ceil() as usize;
      let skip = skip_frames
        .saturating_mul(channels_usize)
        .min(packet.samples.len());
      packet.samples.drain(..skip);
      packet.captured_at = origin;
    }
    if packet.samples.is_empty() {
      continue;
    }
    first_at.get_or_insert(packet.captured_at);
    for sample in packet.samples {
      file
        .write_all(&sample.to_le_bytes())
        .map_err(|error| error.to_string())?;
    }
  }
  file.flush().map_err(|error| error.to_string())?;
  let first_offset_ms = first_at
    .map(|first| {
      first
        .saturating_duration_since(*origin.get().unwrap_or(&first))
        .as_millis()
    })
    .and_then(|value| u64::try_from(value).ok())
    .unwrap_or(0);
  Ok(RawSource {
    channels,
    first_offset_ms,
    path,
    sample_rate,
  })
}

struct MicrophoneCapture {
  sink: RawSink,
  stream: Stream,
}

struct SystemCapture {
  sink: RawSink,
  stop: Arc<AtomicBool>,
  thread: Option<JoinHandle<()>>,
}

pub(super) struct AudioCaptures {
  microphone: Option<MicrophoneCapture>,
  paused: Arc<AtomicBool>,
  system: Vec<SystemCapture>,
}

pub(super) struct AudioFiles {
  pub(super) has_microphone: bool,
  pub(super) has_system_audio: bool,
  microphone: Option<RawSource>,
  system: Vec<RawSource>,
}

impl AudioCaptures {
  pub(super) fn start(
    microphone_id: Option<&str>,
    system: &SystemAudioSelection,
    origin: Arc<OnceLock<Instant>>,
    monitor: Arc<RecordingMonitor>,
    on_failure: FailureReport,
    video_path: &Path,
  ) -> Result<Self, String> {
    let paused = Arc::new(AtomicBool::new(false));
    let microphone = microphone_id
      .map(|device_id| {
        start_microphone(
          device_id,
          sidecar_path(video_path, "microphone", 0),
          Arc::clone(&origin),
          Arc::clone(&paused),
          Arc::clone(&monitor),
          Arc::clone(&on_failure),
        )
      })
      .transpose()?;

    let mut captures = Vec::new();
    if system.enabled {
      let process_ids = if system.process_ids.is_empty() {
        vec![None]
      } else {
        let mut ids = system.process_ids.clone();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(Some).collect()
      };
      for (index, process_id) in process_ids.into_iter().enumerate() {
        captures.push(start_system(
          process_id,
          sidecar_path(video_path, "system", index),
          Arc::clone(&origin),
          Arc::clone(&paused),
          Arc::clone(&monitor),
          Arc::clone(&on_failure),
        )?);
      }
    }
    Ok(Self {
      microphone,
      paused,
      system: captures,
    })
  }

  pub(super) fn pause(&self) {
    self.paused.store(true, Ordering::Release);
  }

  pub(super) fn resume(&self) {
    self.paused.store(false, Ordering::Release);
  }

  pub(super) fn finish(mut self) -> Result<AudioFiles, String> {
    let microphone = if let Some(capture) = self.microphone.take() {
      drop(capture.stream);
      Some(capture.sink.finish()?)
    } else {
      None
    };
    let mut system = Vec::with_capacity(self.system.len());
    for mut capture in self.system.drain(..) {
      capture.stop.store(true, Ordering::Release);
      if let Some(thread) = capture.thread.take() {
        let _ = thread.join();
      }
      system.push(capture.sink.finish()?);
    }
    Ok(AudioFiles {
      has_microphone: microphone.is_some(),
      has_system_audio: !system.is_empty(),
      microphone,
      system,
    })
  }
}

fn sidecar_path(video: &Path, kind: &str, index: usize) -> PathBuf {
  let stem = video
    .file_stem()
    .and_then(|value| value.to_str())
    .unwrap_or("recording");
  video.with_file_name(format!("{stem}.{kind}-{index}.f32"))
}

fn start_microphone(
  device_id: &str,
  path: PathBuf,
  origin: Arc<OnceLock<Instant>>,
  paused: Arc<AtomicBool>,
  monitor: Arc<RecordingMonitor>,
  on_failure: FailureReport,
) -> Result<MicrophoneCapture, String> {
  let source = microphone::Source::resolve(device_id)?;
  let format = source.format();
  let sink = RawSink::start(path, format.sample_rate, format.channels, origin)?;
  let sender = sink.sender()?;
  let callback_monitor = Arc::clone(&monitor);
  let stream = source.start(
    Arc::new(move |buffer| {
      callback_monitor.send_microphone(&buffer.samples);
      if !paused.load(Ordering::Acquire) {
        let _ = sender.try_send(Packet {
          captured_at: buffer.captured_at,
          samples: buffer.samples,
        });
      }
    }),
    on_failure,
  )?;
  Ok(MicrophoneCapture { sink, stream })
}

fn start_system(
  process_id: Option<u32>,
  path: PathBuf,
  origin: Arc<OnceLock<Instant>>,
  paused: Arc<AtomicBool>,
  monitor: Arc<RecordingMonitor>,
  on_failure: FailureReport,
) -> Result<SystemCapture, String> {
  let sink = RawSink::start(path, SYSTEM_SAMPLE_RATE, SYSTEM_CHANNELS, origin)?;
  let sender = sink.sender()?;
  let stop = Arc::new(AtomicBool::new(false));
  let thread_stop = Arc::clone(&stop);
  let (ready_tx, ready_rx) = mpsc::sync_channel(1);
  let thread = thread::Builder::new()
    .name("orbit-windows-system-audio".to_owned())
    .spawn(move || {
      capture_system(
        process_id,
        thread_stop,
        paused,
        sender,
        monitor,
        ready_tx,
        on_failure,
      );
    })
    .map_err(|error| error.to_string())?;
  ready_rx
    .recv_timeout(Duration::from_secs(10))
    .map_err(|_| "Timed out starting Windows system audio".to_owned())??;
  Ok(SystemCapture {
    sink,
    stop,
    thread: Some(thread),
  })
}

fn capture_system(
  process_id: Option<u32>,
  stop: Arc<AtomicBool>,
  paused: Arc<AtomicBool>,
  sender: mpsc::SyncSender<Packet>,
  monitor: Arc<RecordingMonitor>,
  ready: mpsc::SyncSender<Result<(), String>>,
  on_failure: FailureReport,
) {
  if initialize_mta().is_err() {
    let _ = ready.send(Err("Could not initialize Windows system audio".to_owned()));
    return;
  }
  let initialized = initialize_system_client(process_id);
  let (audio_client, capture_client, event) = match initialized {
    Ok(value) => {
      let _ = ready.send(Ok(()));
      value
    }
    Err(error) => {
      let _ = ready.send(Err(error));
      wasapi::deinitialize();
      return;
    }
  };
  if let Err(error) =
    capture_system_packets(&capture_client, &event, &stop, &paused, &sender, &monitor)
  {
    on_failure(format!("System audio recording stopped: {error}"));
  }
  let _ = audio_client.stop_stream();
  wasapi::deinitialize();
}

fn initialize_system_client(
  process_id: Option<u32>,
) -> Result<(AudioClient, AudioCaptureClient, Handle), String> {
  let format = WaveFormat::new(
    32,
    32,
    &SampleType::Float,
    SYSTEM_SAMPLE_RATE as usize,
    SYSTEM_CHANNELS as usize,
    None,
  );
  let (mut client, direction) = if let Some(process_id) = process_id {
    (
      AudioClient::new_application_loopback_client(process_id, true)
        .map_err(|error| error.to_string())?,
      Direction::Capture,
    )
  } else {
    let device = DeviceEnumerator::new()
      .and_then(|enumerator| enumerator.get_default_device(&Direction::Render))
      .map_err(|error| error.to_string())?;
    (
      device
        .get_iaudioclient()
        .map_err(|error| error.to_string())?,
      Direction::Capture,
    )
  };
  client
    .initialize_client(
      &format,
      &direction,
      &StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
      },
    )
    .map_err(|error| error.to_string())?;
  let event = client
    .set_get_eventhandle()
    .map_err(|error| error.to_string())?;
  let capture = client
    .get_audiocaptureclient()
    .map_err(|error| error.to_string())?;
  client.start_stream().map_err(|error| error.to_string())?;
  Ok((client, capture, event))
}

fn capture_system_packets(
  capture: &AudioCaptureClient,
  event: &Handle,
  stop: &AtomicBool,
  paused: &AtomicBool,
  sender: &mpsc::SyncSender<Packet>,
  monitor: &RecordingMonitor,
) -> Result<(), String> {
  let mut bytes = Vec::new();
  while !stop.load(Ordering::Acquire) {
    while capture
      .get_next_packet_size()
      .map_err(|error| error.to_string())?
      .is_some_and(|frames| frames > 0)
    {
      let frames = capture
        .get_next_packet_size()
        .map_err(|error| error.to_string())?
        .unwrap_or(0) as usize;
      bytes.resize(frames * usize::from(SYSTEM_CHANNELS) * size_of::<f32>(), 0);
      let (read, info) = capture
        .read_from_device(&mut bytes)
        .map_err(|error| error.to_string())?;
      let sample_count = read as usize * usize::from(SYSTEM_CHANNELS);
      let samples = if info.flags.silent {
        vec![0.0; sample_count]
      } else {
        bytes[..sample_count * size_of::<f32>()]
          .chunks_exact(4)
          .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap_or([0; 4])))
          .collect::<Vec<_>>()
      };
      monitor.send_system_audio(&samples);
      if !paused.load(Ordering::Acquire) {
        let duration = Duration::from_secs_f64(read as f64 / f64::from(SYSTEM_SAMPLE_RATE));
        let now = Instant::now();
        let captured_at = now.checked_sub(duration).unwrap_or(now);
        let _ = sender.try_send(Packet {
          captured_at,
          samples,
        });
      }
    }
    let _ = event.wait_for_event(50);
  }
  Ok(())
}

pub(super) fn mux(video: &Path, duration_ms: u64, files: AudioFiles) -> Result<(), String> {
  mux_file(video, duration_ms, files, true)
}

pub(super) fn mux_audio_only(
  output_path: &Path,
  duration_ms: u64,
  files: AudioFiles,
) -> Result<(), String> {
  mux_file(output_path, duration_ms, files, false)
}

fn mux_file(
  destination: &Path,
  duration_ms: u64,
  files: AudioFiles,
  include_video: bool,
) -> Result<(), String> {
  let parent = destination
    .parent()
    .ok_or_else(|| "The recording has no directory".to_owned())?;
  let stem = destination
    .file_stem()
    .and_then(|value| value.to_str())
    .unwrap_or("recording");
  let output = parent.join(format!("{stem}.audio-mux.mp4"));
  let mut command = Command::new(crate::exports::ffmpeg_path());
  command.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"]);
  if include_video {
    command.arg("-i").arg(destination);
  }
  let mut input_index = usize::from(include_video);
  let mut system_inputs = Vec::new();
  for source in files
    .system
    .iter()
    .filter(|source| source.path.metadata().is_ok_and(|m| m.len() > 0))
  {
    add_raw_input(&mut command, source);
    system_inputs.push((input_index, source.first_offset_ms));
    input_index += 1;
  }
  let microphone_input = files.microphone.as_ref().and_then(|source| {
    source
      .path
      .metadata()
      .ok()
      .filter(|metadata| metadata.len() > 0)?;
    add_raw_input(&mut command, source);
    let value = (input_index, source.first_offset_ms);
    input_index += 1;
    Some(value)
  });
  let duration = format!("{}.{:03}", duration_ms / 1_000, duration_ms % 1_000);
  let mut filter = String::new();
  let mut maps = Vec::new();
  if files.has_system_audio {
    append_track_filter(
      &mut command,
      &mut input_index,
      &mut filter,
      &system_inputs,
      "system",
      &duration,
    );
    maps.push(("[system]", "System audio"));
  }
  if files.has_microphone {
    let inputs = microphone_input.into_iter().collect::<Vec<_>>();
    append_track_filter(
      &mut command,
      &mut input_index,
      &mut filter,
      &inputs,
      "microphone",
      &duration,
    );
    maps.push(("[microphone]", "Microphone"));
  }
  if include_video {
    command.args(["-map", "0:v:0", "-c:v", "copy"]);
  }
  if !filter.is_empty() {
    command.args(["-filter_complex", filter.trim_end_matches(';')]);
  }
  for (position, (map, title)) in maps.iter().enumerate() {
    command.args(["-map", map]);
    command.arg(format!("-metadata:s:a:{position}"));
    command.arg(format!("title={title}"));
  }
  command.args([
    "-c:a",
    "aac",
    "-b:a",
    "192k",
    "-t",
    &duration,
    "-movflags",
    "+faststart",
  ]);
  command.arg(&output);
  let result = command
    .output()
    .map_err(|error| format!("FFmpeg could not mux recording audio: {error}"))?;
  if !result.status.success() {
    return Err(format!(
      "FFmpeg could not finish recording audio: {}",
      String::from_utf8_lossy(&result.stderr).trim()
    ));
  }
  if include_video {
    let backup = parent.join(format!("{stem}.video-backup.mp4"));
    fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    if let Err(error) = fs::rename(&output, destination) {
      let _ = fs::rename(&backup, destination);
      return Err(error.to_string());
    }
    let _ = fs::remove_file(backup);
  } else {
    fs::rename(&output, destination).map_err(|error| error.to_string())?;
  }
  for source in files.system.into_iter().chain(files.microphone) {
    let _ = fs::remove_file(source.path);
  }
  Ok(())
}

fn add_raw_input(command: &mut Command, source: &RawSource) {
  command.args(["-f", "f32le", "-ar"]);
  command.arg(source.sample_rate.to_string());
  command.arg("-ac");
  command.arg(source.channels.to_string());
  command.arg("-i");
  command.arg(&source.path);
}

fn append_track_filter(
  command: &mut Command,
  input_index: &mut usize,
  filter: &mut String,
  inputs: &[(usize, u64)],
  label: &str,
  duration: &str,
) {
  if inputs.is_empty() {
    command.args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo"]);
    let index = *input_index;
    *input_index += 1;
    filter.push_str(&format!("[{index}:a]atrim=duration={duration}[{label}];"));
    return;
  }
  for (position, (index, delay)) in inputs.iter().enumerate() {
    filter.push_str(&format!(
      "[{index}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,adelay={delay}:all=1,apad,atrim=duration={duration}[{label}{position}];"
    ));
  }
  if inputs.len() == 1 {
    filter.push_str(&format!("[{label}0]anull[{label}];"));
  } else {
    for position in 0..inputs.len() {
      filter.push_str(&format!("[{label}{position}]"));
    }
    filter.push_str(&format!(
      "amix=inputs={}:duration=longest:normalize=0,alimiter=limit=0.98[{label}];",
      inputs.len()
    ));
  }
}
