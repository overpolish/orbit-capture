// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! CPAL microphone capture, kept independent from the container writer.
//!
//! The real-time callback only timestamps and copies one bounded device buffer.
//! CoreMedia conversion and AAC encoding happen later on the writer thread.

use std::sync::Arc;
use std::time::Instant;

use cpal::{
  traits::{DeviceTrait, StreamTrait},
  Device, FromSample, InputCallbackInfo, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
};

#[derive(Clone, Copy, Debug)]
pub struct Format {
  pub channels: u16,
  pub sample_rate: u32,
}

pub struct Buffer {
  pub captured_at: Instant,
  pub samples: Vec<f32>,
}

pub struct Source {
  config: StreamConfig,
  device: Device,
  sample_format: SampleFormat,
}

impl Source {
  pub fn resolve(device_id: &str) -> Result<Self, String> {
    let (device, config, sample_format) =
      crate::recording_inputs::resolve_microphone(Some(device_id))?;
    Ok(Self {
      config,
      device,
      sample_format,
    })
  }

  pub const fn format(&self) -> Format {
    Format {
      channels: self.config.channels,
      sample_rate: self.config.sample_rate,
    }
  }

  pub fn start(
    self,
    on_buffer: Arc<dyn Fn(Buffer) + Send + Sync>,
    on_error: Arc<dyn Fn(String) + Send + Sync>,
  ) -> Result<Stream, String> {
    let stream = match self.sample_format {
      SampleFormat::F32 => build_stream::<f32>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::F64 => build_stream::<f64>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::I8 => build_stream::<i8>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::I16 => build_stream::<i16>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::I24 => {
        build_stream::<cpal::I24>(&self.device, &self.config, on_buffer, on_error)
      }
      SampleFormat::I32 => build_stream::<i32>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::I64 => build_stream::<i64>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::U8 => build_stream::<u8>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::U16 => build_stream::<u16>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::U24 => {
        build_stream::<cpal::U24>(&self.device, &self.config, on_buffer, on_error)
      }
      SampleFormat::U32 => build_stream::<u32>(&self.device, &self.config, on_buffer, on_error),
      SampleFormat::U64 => build_stream::<u64>(&self.device, &self.config, on_buffer, on_error),
      _ => {
        return Err(format!(
          "Unsupported microphone sample format: {}",
          self.sample_format
        ))
      }
    }?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(stream)
  }
}

fn build_stream<T>(
  device: &Device,
  config: &StreamConfig,
  on_buffer: Arc<dyn Fn(Buffer) + Send + Sync>,
  on_error: Arc<dyn Fn(String) + Send + Sync>,
) -> Result<Stream, String>
where
  T: SizedSample,
  f32: FromSample<T>,
{
  device
    .build_input_stream(
      *config,
      move |data: &[T], info: &InputCallbackInfo| {
        let now = Instant::now();
        let timestamp = info.timestamp();
        let latency = timestamp
          .callback
          .saturating_duration_since(timestamp.capture);
        let captured_at = now.checked_sub(latency).unwrap_or(now);
        let samples = data.iter().copied().map(f32::from_sample).collect();
        on_buffer(Buffer {
          captured_at,
          samples,
        });
      },
      move |error| on_error(error.to_string()),
      None,
    )
    .map_err(|error| error.to_string())
}
