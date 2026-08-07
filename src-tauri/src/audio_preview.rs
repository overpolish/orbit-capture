use std::sync::Mutex;

use cpal::{
  traits::{DeviceTrait, HostTrait, StreamTrait},
  Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use serde::{Deserialize, Serialize};
use tauri::{ipc::Channel, AppHandle, Manager};

#[cfg(target_os = "macos")]
mod platform;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioPreviewKind {
  Microphone,
  System,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum AudioPreviewEvent {
  Signal { decibels: f32 },
  Error { message: String },
}

#[derive(Default)]
struct AudioPreviewManager {
  microphone: Option<Stream>,
  system: Option<SystemAudioPreview>,
}

enum SystemAudioPreview {
  Cpal {
    _stream: Stream,
  },
  #[cfg(target_os = "macos")]
  ScreenCaptureKit {
    _preview: platform::FilteredAudioPreview,
  },
}

impl AudioPreviewManager {
  fn replace_microphone(&mut self, stream: Stream) {
    self.microphone = Some(stream);
  }

  fn replace_system(&mut self, stream: SystemAudioPreview) {
    self.system = Some(stream);
  }

  fn stop(&mut self, kind: AudioPreviewKind) {
    match kind {
      AudioPreviewKind::Microphone => {
        self.microphone.take();
      }
      AudioPreviewKind::System => {
        self.system.take();
      }
    }
  }

  fn stop_all(&mut self) {
    self.microphone.take();
    self.system.take();
  }
}

#[derive(Default)]
pub struct AudioPreviewState(Mutex<AudioPreviewManager>);

struct LevelAccumulator {
  peak: f64,
  samples: usize,
  samples_per_update: usize,
}

impl LevelAccumulator {
  fn new(config: &StreamConfig) -> Self {
    Self::with_format(config.sample_rate, config.channels)
  }

  fn with_format(sample_rate: u32, channels: u16) -> Self {
    let samples_per_update = (sample_rate as usize * usize::from(channels) / 30).max(1);
    Self {
      peak: 0.0,
      samples: 0,
      samples_per_update,
    }
  }

  fn push<I, F>(&mut self, samples: I, mut on_level: F)
  where
    I: IntoIterator<Item = f64>,
    F: FnMut(f32),
  {
    for sample in samples {
      self.peak = self.peak.max(sample.abs());
      self.samples += 1;

      if self.samples == self.samples_per_update {
        let decibels = 20.0 * self.peak.max(1e-8).log10();
        self.peak = 0.0;
        self.samples = 0;
        on_level(decibels as f32);
      }
    }
  }
}

fn microphone(device_id: Option<&str>) -> Result<(Device, StreamConfig, SampleFormat), String> {
  let host = cpal::default_host();
  let device = match device_id {
    Some(device_id) => host
      .input_devices()
      .map_err(|error| error.to_string())?
      .find(|device| {
        device
          .id()
          .is_ok_and(|candidate| candidate.to_string() == device_id)
      })
      .ok_or_else(|| "The selected microphone is no longer available".to_owned())?,
    None => host
      .default_input_device()
      .ok_or_else(|| "No default microphone is available".to_owned())?,
  };
  let config = device
    .default_input_config()
    .map_err(|error| error.to_string())?;
  let sample_format = config.sample_format();
  Ok((device, config.into(), sample_format))
}

fn system_audio() -> Result<(Device, StreamConfig, SampleFormat), String> {
  let device = cpal::default_host()
    .default_output_device()
    .ok_or_else(|| "No default audio output is available".to_owned())?;
  let config = device
    .default_output_config()
    .map_err(|error| error.to_string())?;
  let sample_format = config.sample_format();
  Ok((device, config.into(), sample_format))
}

fn build_sample_stream<T>(
  device: &Device,
  config: &StreamConfig,
  channel: Channel<AudioPreviewEvent>,
) -> Result<Stream, String>
where
  T: SizedSample,
  f64: FromSample<T>,
{
  let mut level = LevelAccumulator::new(config);
  let error_channel = channel.clone();
  device
    .build_input_stream(
      *config,
      move |data: &[T], _| {
        level.push(data.iter().copied().map(f64::from_sample), |decibels| {
          let _ = channel.send(AudioPreviewEvent::Signal { decibels });
        });
      },
      move |error| {
        let _ = error_channel.send(AudioPreviewEvent::Error {
          message: error.to_string(),
        });
      },
      None,
    )
    .map_err(|error| error.to_string())
}

fn build_stream(
  device: &Device,
  config: &StreamConfig,
  sample_format: SampleFormat,
  channel: Channel<AudioPreviewEvent>,
) -> Result<Stream, String> {
  let stream = match sample_format {
    SampleFormat::F32 => build_sample_stream::<f32>(device, config, channel),
    SampleFormat::F64 => build_sample_stream::<f64>(device, config, channel),
    SampleFormat::I8 => build_sample_stream::<i8>(device, config, channel),
    SampleFormat::I16 => build_sample_stream::<i16>(device, config, channel),
    SampleFormat::I24 => build_sample_stream::<cpal::I24>(device, config, channel),
    SampleFormat::I32 => build_sample_stream::<i32>(device, config, channel),
    SampleFormat::I64 => build_sample_stream::<i64>(device, config, channel),
    SampleFormat::U8 => build_sample_stream::<u8>(device, config, channel),
    SampleFormat::U16 => build_sample_stream::<u16>(device, config, channel),
    SampleFormat::U24 => build_sample_stream::<cpal::U24>(device, config, channel),
    SampleFormat::U32 => build_sample_stream::<u32>(device, config, channel),
    SampleFormat::U64 => build_sample_stream::<u64>(device, config, channel),
    _ => return Err(format!("Unsupported audio sample format: {sample_format}")),
  }?;

  stream.play().map_err(|error| error.to_string())?;
  Ok(stream)
}

#[tauri::command]
pub async fn start_audio_preview(
  state: tauri::State<'_, AudioPreviewState>,
  kind: AudioPreviewKind,
  device_id: Option<String>,
  application_ids: Option<Vec<String>>,
  channel: Channel<AudioPreviewEvent>,
) -> Result<(), String> {
  match kind {
    AudioPreviewKind::Microphone => {
      let (device, config, sample_format) = microphone(device_id.as_deref())?;
      let stream = build_stream(&device, &config, sample_format, channel)?;
      state
        .0
        .lock()
        .map_err(|_| "Audio preview state is unavailable".to_owned())?
        .replace_microphone(stream);
    }
    AudioPreviewKind::System => {
      let application_ids = application_ids.unwrap_or_default();
      #[cfg(target_os = "macos")]
      let stream = if application_ids.is_empty() {
        let (device, config, sample_format) = system_audio()?;
        SystemAudioPreview::Cpal {
          _stream: build_stream(&device, &config, sample_format, channel)?,
        }
      } else {
        SystemAudioPreview::ScreenCaptureKit {
          _preview: platform::start_filtered_audio_preview(application_ids, channel).await?,
        }
      };
      #[cfg(not(target_os = "macos"))]
      let stream = {
        if !application_ids.is_empty() {
          return Err("Application audio preview is not yet available on this platform".into());
        }
        let (device, config, sample_format) = system_audio()?;
        SystemAudioPreview::Cpal {
          _stream: build_stream(&device, &config, sample_format, channel)?,
        }
      };
      state
        .0
        .lock()
        .map_err(|_| "Audio preview state is unavailable".to_owned())?
        .replace_system(stream);
    }
  }
  Ok(())
}

#[tauri::command]
pub fn stop_audio_preview(
  state: tauri::State<'_, AudioPreviewState>,
  kind: AudioPreviewKind,
) -> Result<(), String> {
  state
    .0
    .lock()
    .map_err(|_| "Audio preview state is unavailable".to_owned())?
    .stop(kind);
  Ok(())
}

pub fn stop_all(app: &AppHandle) {
  if let Ok(mut manager) = app.state::<AudioPreviewState>().0.lock() {
    manager.stop_all();
  }
}

#[cfg(test)]
mod tests {
  use super::LevelAccumulator;
  use cpal::{BufferSize, StreamConfig};

  fn accumulator(samples_per_update: u32) -> LevelAccumulator {
    LevelAccumulator::new(&StreamConfig {
      buffer_size: BufferSize::Default,
      channels: 1,
      sample_rate: samples_per_update * 30,
    })
  }

  #[test]
  fn emits_at_thirty_frames_per_second() {
    let mut level = accumulator(4);
    let mut output = Vec::new();
    level.push([0.5, 0.5, 0.5], |value| output.push(value));
    assert!(output.is_empty());
    level.push([0.5], |value| output.push(value));
    assert_eq!(output.first().map(|value| value.round()), Some(-6.0));
  }

  #[test]
  fn short_transients_reach_the_sample_peak() {
    let mut level = accumulator(4);
    let mut output = Vec::new();
    level.push([1.0, 0.0, 0.0, 0.0], |value| output.push(value));
    assert_eq!(output, [0.0]);
  }

  #[test]
  fn resets_after_emitting_a_level() {
    let mut level = accumulator(2);
    let mut output = Vec::new();
    level.push([1.0, 1.0], |value| output.push(value));
    output.clear();
    level.push([0.1, 0.1], |value| output.push(value));
    assert_eq!(output.first().map(|value| value.round()), Some(-20.0));
  }

  #[test]
  fn preserves_samples_beyond_an_update_boundary() {
    let mut level = accumulator(2);
    let mut output = Vec::new();
    level.push([1.0, 1.0, 0.1, 0.1], |value| output.push(value));
    assert_eq!(output.len(), 2);
    assert_eq!(output[0], 0.0);
    assert_eq!(output[1].round(), -20.0);
  }

  #[test]
  fn clamps_silence_to_a_finite_floor() {
    let mut level = accumulator(1);
    let mut output = Vec::new();
    level.push([0.0], |value| output.push(value));
    assert_eq!(output, [-160.0]);
  }
}
