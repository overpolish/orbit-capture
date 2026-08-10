// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Audio-only capture has its own audio-led writer. It never opens
//! a video encoder or creates a placeholder video track.

mod lifecycle;
mod samples;

use std::collections::VecDeque;
use std::path::PathBuf;

use cidre::av;

use super::media::{
  microphone_audio_settings, microphone_buffer_from_origin, microphone_format_description,
  microphone_sample_buffer, nanos, system_audio_settings, time_to_ns,
};
use super::*;
use crate::recording::encoding::{FailureReport, Timeline};
use crate::recording::PrimaryRecordingKind;

pub(super) struct AudioWriter {
  base: Instant,
  failed: Option<String>,
  microphone_end_ns: Option<i64>,
  microphone_format: Option<MicrophoneFormat>,
  microphone_format_description: Option<arc::R<cm::AudioFormatDesc>>,
  microphone_input: Option<arc::R<av::AssetWriterInput>>,
  on_failure: FailureReport,
  origin: Option<Instant>,
  path: PathBuf,
  pending_microphone: VecDeque<MicrophoneBuffer>,
  pending_system_audio: VecDeque<AudioSample>,
  stats: Arc<CaptureStats>,
  system_audio_end_ns: Option<i64>,
  system_audio_format_description: Option<arc::R<cm::AudioFormatDesc>>,
  system_audio_input: Option<arc::R<av::AssetWriterInput>>,
  timeline: Timeline,
  writer: arc::R<av::AssetWriter>,
}

impl AudioWriter {
  pub(super) fn new(
    path: PathBuf,
    system_audio: bool,
    microphone_format: Option<MicrophoneFormat>,
    stats: Arc<CaptureStats>,
    on_failure: FailureReport,
  ) -> Result<Self, String> {
    let location = path
      .to_str()
      .ok_or_else(|| "The recording's location cannot be written as text".to_owned())?;
    let url = ns::Url::with_fs_path_str(location, false);
    let container = Container::quicktime_fragmented();
    let mut writer = av::AssetWriter::with_url_and_file_type(&url, container.format.file_type())
      .map_err(|error| error.to_string())?;
    if let Some(interval) = container.fragment_interval {
      writer.set_movie_fragment_interval(interval);
    }

    let (system_audio_input, system_audio_format_description) = if system_audio {
      let mut input = av::AssetWriterInput::with_media_type_and_output_settings(
        av::MediaType::audio(),
        Some(&system_audio_settings()),
      )
      .map_err(|error| error.to_string())?;
      input.set_expects_media_data_in_real_time(true);
      writer
        .add_input(&input)
        .map_err(|error| error.to_string())?;
      let description = microphone_format_description(MicrophoneFormat {
        channels: SYSTEM_AUDIO_CHANNELS as u16,
        sample_rate: SYSTEM_AUDIO_SAMPLE_RATE as u32,
      })?;
      (Some(input), Some(description))
    } else {
      (None, None)
    };
    let (microphone_input, microphone_format_description) = if let Some(format) = microphone_format
    {
      let mut input = av::AssetWriterInput::with_media_type_and_output_settings(
        av::MediaType::audio(),
        Some(&microphone_audio_settings(format)),
      )
      .map_err(|error| error.to_string())?;
      input.set_expects_media_data_in_real_time(true);
      writer
        .add_input(&input)
        .map_err(|error| error.to_string())?;
      (Some(input), Some(microphone_format_description(format)?))
    } else {
      (None, None)
    };
    if !writer.start_writing() {
      return Err(asset_writer_error(
        &writer,
        "The audio recording could not be started",
      ));
    }
    writer.start_session_at_src_time(cm::Time::zero());

    Ok(Self {
      base: Instant::now(),
      failed: None,
      microphone_end_ns: None,
      microphone_format,
      microphone_format_description,
      microphone_input,
      on_failure,
      origin: None,
      path,
      pending_microphone: VecDeque::new(),
      pending_system_audio: VecDeque::new(),
      stats,
      system_audio_end_ns: None,
      system_audio_format_description,
      system_audio_input,
      timeline: Timeline::default(),
      writer,
    })
  }

  fn elapsed_ns(&self, at: Instant) -> i64 {
    i64::try_from(at.saturating_duration_since(self.base).as_nanos()).unwrap_or(i64::MAX)
  }

  fn begin(&mut self, at: Instant) -> Result<(), String> {
    self.origin = Some(at);
    let wall_ns = self.elapsed_ns(at);
    self.timeline.start_at(0, wall_ns);
    for sample in std::mem::take(&mut self.pending_system_audio) {
      self.append_system_audio(sample);
    }
    if let Some(format) = self.microphone_format {
      for buffer in std::mem::take(&mut self.pending_microphone) {
        self.append_pcm(buffer, format, false);
      }
    }
    self.pad_tracks_to(0);
    self.failed.clone().map_or(Ok(()), Err)
  }

  fn fail(&mut self, error: String) {
    if self.failed.is_none() {
      (self.on_failure)(error.clone());
      self.failed = Some(error);
    }
  }
}
