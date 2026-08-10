// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

impl Writer {
  pub(super) fn flush_system_audio_preroll(&mut self) {
    let pending = std::mem::take(&mut self.pending_system_audio);
    for sample in pending {
      self.append_system_audio_from_origin(sample);
    }
  }

  pub(super) fn append_system_audio_from_origin(&mut self, sample: AudioSample) {
    let Some(origin_source_ns) = self.origin_source_ns else {
      return;
    };
    if let Some(sample) = audio_sample_from_origin(sample, origin_source_ns) {
      self.append_mapped_system_audio(&sample);
    }
  }

  pub(super) fn append_mapped_system_audio(&mut self, sample: &AudioSample) {
    let mut pts = self
      .timeline
      .media_pts_ns(sample.source_ns, self.elapsed_ns(sample.wall));
    if let Some(last) = self.last_system_audio_pts_ns {
      pts = pts.max(last.saturating_add(1));
    }
    self.append_system_audio(sample, pts);
  }

  pub(super) fn flush_microphone_preroll(&mut self) {
    let pending = std::mem::take(&mut self.pending_microphone);
    for microphone in pending {
      self.append_microphone_from_origin(microphone);
    }
  }

  pub(super) fn append_microphone_from_origin(&mut self, microphone: MicrophoneBuffer) {
    let (Some(origin), Some(format)) = (self.origin_wall, self.microphone_format) else {
      return;
    };
    let Some(microphone) = microphone_buffer_from_origin(microphone, origin, format) else {
      return;
    };
    let mut pts = self
      .timeline
      .wall_pts_ns(self.elapsed_ns(microphone.captured_at));
    if let Some(last) = self.last_microphone_pts_ns {
      pts = pts.max(last.saturating_add(1));
    }
    self.append_microphone(&microphone, format, pts);
  }

  /// Appends one frame, reporting whether it actually landed.
  ///
  /// A writer that has failed is never asked again: AVFoundation refuses every
  /// subsequent sample, and asking it sixty times a second turns one fault
  /// into a flood of identical complaints.
  pub(super) fn append(&mut self, frame: &Frame, pts_ns: i64) -> bool {
    if self.failed.is_some() {
      self.stats.rejected.fetch_add(1, Ordering::Relaxed);
      return false;
    }
    if !self.video_input_is_ready() {
      self.stats.not_ready.fetch_add(1, Ordering::Relaxed);
      return false;
    }

    match self
      .adaptor
      .append_pixel_buf_with_pts(&frame.buf, nanos(pts_ns))
    {
      Ok(true) => {
        self.stats.appended.fetch_add(1, Ordering::Relaxed);
        self.rejection_streak = 0;
        self.last_appended_ns = Some(pts_ns);
        true
      }
      Ok(false) => {
        self.refused(writer_error(
          &self.writer,
          "the recording could not continue",
        ));
        false
      }
      Err(error) => {
        self.refused(error.to_string());
        false
      }
    }
  }

  /// A screen frame may be replaced by the next changed frame, so it is
  /// dropped immediately under backpressure. Every camera frame represents a
  /// point in continuous motion: wait on this writer thread while the bounded
  /// capture queue absorbs a short hardware-encoder stall. The capture
  /// callback itself always remains non-blocking.
  fn video_input_is_ready(&self) -> bool {
    if self.input.is_ready_for_more_media_data() {
      return true;
    }
    if !matches!(self.source, VideoSource::Camera) {
      return false;
    }

    let deadline = Instant::now() + CAMERA_ENCODER_WAIT;
    while Instant::now() < deadline {
      std::thread::sleep(CAMERA_ENCODER_POLL);
      if self.input.is_ready_for_more_media_data() {
        return true;
      }
    }
    false
  }

  pub(super) fn append_system_audio(&mut self, sample: &AudioSample, pts_ns: i64) {
    if self.failed.is_some() || self.system_audio_input.is_none() {
      return;
    }
    if !self
      .system_audio_input
      .as_ref()
      .is_some_and(|input| input.is_ready_for_more_media_data())
    {
      self.stats.audio_not_ready.fetch_add(1, Ordering::Relaxed);
      return;
    }

    let retimed = match audio_sample_with_pts(&sample.buf, pts_ns) {
      Ok(sample) => sample,
      Err(error) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(error);
        return;
      }
    };
    let duration_ns = time_to_ns(sample.buf.duration()).unwrap_or_default();
    let result = self
      .system_audio_input
      .as_mut()
      .expect("checked above")
      .append_sample_buf(&retimed);
    match result {
      Ok(true) => {
        self.last_system_audio_pts_ns = Some(pts_ns);
        self.system_audio_end_ns = Some(pts_ns.saturating_add(duration_ns));
      }
      Ok(false) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(writer_error(
          &self.writer,
          "the system-audio track could not continue",
        ));
      }
      Err(error) => {
        self.stats.audio_rejected.fetch_add(1, Ordering::Relaxed);
        self.refused(error.to_string());
      }
    }
  }

  pub(super) fn append_microphone(
    &mut self,
    microphone: &MicrophoneBuffer,
    format: MicrophoneFormat,
    pts_ns: i64,
  ) {
    if self.failed.is_some() || self.microphone_input.is_none() {
      return;
    }
    if !self
      .microphone_input
      .as_ref()
      .is_some_and(|input| input.is_ready_for_more_media_data())
    {
      self
        .stats
        .microphone_not_ready
        .fetch_add(1, Ordering::Relaxed);
      return;
    }

    let Some(description) = self.microphone_format_description.as_ref() else {
      return;
    };
    let sample = match microphone_sample_buffer(microphone, format, description, pts_ns) {
      Ok(sample) => sample,
      Err(error) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(error);
        return;
      }
    };
    let duration_ns = time_to_ns(sample.duration()).unwrap_or_default();
    let result = self
      .microphone_input
      .as_mut()
      .expect("checked above")
      .append_sample_buf(&sample);
    match result {
      Ok(true) => {
        self.last_microphone_pts_ns = Some(pts_ns);
        self.microphone_end_ns = Some(pts_ns.saturating_add(duration_ns));
      }
      Ok(false) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(writer_error(
          &self.writer,
          "the microphone track could not continue",
        ));
      }
      Err(error) => {
        self
          .stats
          .microphone_rejected
          .fetch_add(1, Ordering::Relaxed);
        self.refused(error.to_string());
      }
    }
  }
}
