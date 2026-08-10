// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::Ordering;

use super::*;

impl AudioWriter {
  pub(super) fn append_pcm(
    &mut self,
    buffer: MicrophoneBuffer,
    format: MicrophoneFormat,
    system_audio: bool,
  ) {
    if self.failed.is_some() || self.timeline.is_paused() {
      return;
    }
    let Some(origin) = self.origin else { return };
    let Some(buffer) = microphone_buffer_from_origin(buffer, origin, format) else {
      return;
    };
    let mut pts = self
      .timeline
      .wall_pts_ns(self.elapsed_ns(buffer.captured_at));
    let end = if system_audio {
      self.system_audio_end_ns
    } else {
      self.microphone_end_ns
    };
    if let Some(end) = end {
      pts = pts.max(end);
    }
    let description = if system_audio {
      self.system_audio_format_description.as_ref()
    } else {
      self.microphone_format_description.as_ref()
    };
    let Some(description) = description else {
      return;
    };
    let sample = match microphone_sample_buffer(&buffer, format, description, pts) {
      Ok(sample) => sample,
      Err(error) => return self.fail(error),
    };
    let input = if system_audio {
      self.system_audio_input.as_mut()
    } else {
      self.microphone_input.as_mut()
    };
    let Some(input) = input else { return };
    if !input.is_ready_for_more_media_data() {
      if system_audio {
        self.stats.audio_not_ready.fetch_add(1, Ordering::Relaxed);
      } else {
        self
          .stats
          .microphone_not_ready
          .fetch_add(1, Ordering::Relaxed);
      }
      return;
    }
    match input.append_sample_buf(&sample) {
      Ok(true) => {
        let end = pts.saturating_add(time_to_ns(sample.duration()).unwrap_or_default());
        if system_audio {
          self.system_audio_end_ns = Some(end);
        } else {
          self.microphone_end_ns = Some(end);
        }
      }
      Ok(false) => self.fail(asset_writer_error(
        &self.writer,
        "The audio recording could not continue",
      )),
      Err(error) => self.fail(error.to_string()),
    }
  }

  pub(super) fn append_system_audio(&mut self, sample: AudioSample) {
    self.append_pcm(
      MicrophoneBuffer {
        captured_at: sample.wall,
        samples: sample.samples,
      },
      MicrophoneFormat {
        channels: SYSTEM_AUDIO_CHANNELS as u16,
        sample_rate: SYSTEM_AUDIO_SAMPLE_RATE as u32,
      },
      true,
    );
  }

  pub(super) fn pad_tracks_to(&mut self, end_ns: i64) {
    if self.system_audio_input.is_some() && self.system_audio_end_ns.unwrap_or(-1) < end_ns {
      self.append_silence(true, end_ns);
    }
    if self.microphone_input.is_some() && self.microphone_end_ns.unwrap_or(-1) < end_ns {
      self.append_silence(false, end_ns);
    }
  }

  fn append_silence(&mut self, system_audio: bool, end_ns: i64) {
    let format = if system_audio {
      MicrophoneFormat {
        channels: SYSTEM_AUDIO_CHANNELS as u16,
        sample_rate: SYSTEM_AUDIO_SAMPLE_RATE as u32,
      }
    } else if let Some(format) = self.microphone_format {
      format
    } else {
      return;
    };
    let current_end = if system_audio {
      self.system_audio_end_ns.unwrap_or(0)
    } else {
      self.microphone_end_ns.unwrap_or(0)
    };
    let maximum_frames = 1_024_i64;
    let maximum_packet_ns = maximum_frames * NANOS_PER_SEC / i64::from(format.sample_rate);
    let pts = current_end.max(end_ns.saturating_sub(maximum_packet_ns));
    let gap_ns = end_ns.saturating_sub(pts).max(1);
    let frames = usize::try_from(
      gap_ns
        .saturating_mul(i64::from(format.sample_rate))
        .saturating_add(NANOS_PER_SEC - 1)
        / NANOS_PER_SEC,
    )
    .unwrap_or(1)
    .max(1);
    let buffer = MicrophoneBuffer {
      captured_at: self.origin.unwrap_or(self.base),
      samples: vec![0.0; frames * usize::from(format.channels)],
    };
    let description = if system_audio {
      self.system_audio_format_description.as_ref()
    } else {
      self.microphone_format_description.as_ref()
    };
    let Some(description) = description else {
      return;
    };
    let Ok(sample) = microphone_sample_buffer(&buffer, format, description, pts) else {
      return;
    };
    let input = if system_audio {
      self.system_audio_input.as_mut()
    } else {
      self.microphone_input.as_mut()
    };
    let Some(input) = input else { return };
    if input.is_ready_for_more_media_data() && input.append_sample_buf(&sample).is_ok_and(|v| v) {
      if system_audio {
        self.system_audio_end_ns = Some(pts.saturating_add(
          i64::try_from(frames).unwrap_or_default() * NANOS_PER_SEC / i64::from(format.sample_rate),
        ));
      } else {
        self.microphone_end_ns = Some(pts.saturating_add(
          i64::try_from(frames).unwrap_or_default() * NANOS_PER_SEC / i64::from(format.sample_rate),
        ));
      }
    }
  }
}
