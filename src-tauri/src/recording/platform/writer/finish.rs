// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use cidre::cv;

use super::*;
use crate::recording::PrimaryRecordingKind;

impl Writer {
  /// Appends the closing frame, giving a busy encoder a moment to catch up.
  ///
  /// Ordinary frames are dropped the instant the encoder is busy - there is
  /// another one along in sixteen milliseconds. This one is the last there
  /// will ever be, and the movie's length depends on it landing.
  pub(super) fn append_insisting(&mut self, frame: &Frame, pts_ns: i64) {
    for _ in 0..TAIL_APPEND_ATTEMPTS {
      if self.append(frame, pts_ns) || self.failed.is_some() {
        return;
      }
      std::thread::sleep(TAIL_APPEND_WAIT);
    }
  }

  /// Records a refused frame, and tells the user once if they have stopped
  /// landing altogether.
  pub(super) fn refused(&mut self, reason: String) {
    self.stats.rejected.fetch_add(1, Ordering::Relaxed);
    self.rejection_streak += 1;

    // A failed writer never recovers, so there is no point waiting out the
    // streak before saying so. Short of that, a long enough run of refusals
    // means the same thing by another route.
    let hopeless = self.writer.status() == av::AssetWriterStatus::Failed
      || self.rejection_streak >= REJECTION_STREAK_LIMIT;
    if hopeless {
      self.fail(reason);
    }
  }

  /// Latches the failure and reports it exactly once.
  pub(super) fn fail(&mut self, reason: String) {
    if self.failed.is_some() {
      return;
    }
    eprintln!("Recording stopped accepting frames: {reason}");
    (self.on_failure)(reason.clone());
    self.failed = Some(reason);
  }

  pub(super) fn finish(&mut self, at: Instant) -> Result<FinalizeInfo, String> {
    if !self.timeline.has_started() {
      self.writer.cancel_writing();
      return Err("The recording captured no frames".to_owned());
    }

    let stop_ns = self.timeline.stop_pts_ns(self.elapsed_ns(at));
    // Holding the final frame until the true stop time is what gives a
    // recording of a static screen its real duration, so a busy encoder is
    // worth waiting a moment for rather than giving up on.
    let mut tail = self.tail.take();
    if let Some(frame) = &tail {
      self.append_insisting(frame, stop_ns);
    }

    self.input.mark_as_finished();
    if let Some(input) = self.system_audio_input.as_mut() {
      input.mark_as_finished();
    }
    if let Some(input) = self.microphone_input.as_mut() {
      input.mark_as_finished();
    }
    // The session ends exactly where the media ends. Ending it any later
    // leaves the movie claiming a duration it has no sample to fill, and the
    // writer refuses the whole file for it - which is what a skipped final
    // frame used to cause, intermittently and only at the very end.
    let end_ns = self
      .last_appended_ns
      .unwrap_or(0)
      .max(self.system_audio_end_ns.unwrap_or(0))
      .max(self.microphone_end_ns.unwrap_or(0));
    self
      .writer
      .end_session_at_src_time(nanos(end_ns))
      .map_err(|error| error.to_string())?;
    self.writer.finish_writing();

    if self.writer.status() != av::AssetWriterStatus::Completed {
      return Err(writer_error(
        &self.writer,
        "The recording could not be saved",
      ));
    }

    let capture_dropped = self.stats.capture_dropped.load(Ordering::Relaxed);
    let dropped = self.stats.dropped.load(Ordering::Relaxed);
    let not_ready = self.stats.not_ready.load(Ordering::Relaxed);
    if capture_dropped > 0 || dropped > 0 || not_ready > 0 {
      eprintln!(
        "Recording dropped {capture_dropped} frames at the capture device, {dropped} at the capture queue, and {not_ready} at the encoder"
      );
    }
    let audio_dropped = self.stats.audio_dropped.load(Ordering::Relaxed);
    let audio_not_ready = self.stats.audio_not_ready.load(Ordering::Relaxed);
    let audio_rejected = self.stats.audio_rejected.load(Ordering::Relaxed);
    if audio_dropped > 0 || audio_not_ready > 0 || audio_rejected > 0 {
      eprintln!(
        "Recording dropped {audio_dropped} system-audio buffers at the capture queue, {audio_not_ready} at the encoder, and rejected {audio_rejected}"
      );
    }
    let microphone_dropped = self.stats.microphone_dropped.load(Ordering::Relaxed);
    let microphone_not_ready = self.stats.microphone_not_ready.load(Ordering::Relaxed);
    let microphone_rejected = self.stats.microphone_rejected.load(Ordering::Relaxed);
    if microphone_dropped > 0 || microphone_not_ready > 0 || microphone_rejected > 0 {
      eprintln!(
        "Recording dropped {microphone_dropped} microphone buffers at the capture queue, {microphone_not_ready} at the encoder, and rejected {microphone_rejected}"
      );
    }

    Ok(FinalizeInfo {
      camera: None,
      has_microphone: self.microphone_input.is_some(),
      has_system_audio: self.system_audio_input.is_some(),
      duration_ms: u64::try_from(end_ns / NANOS_PER_MS).unwrap_or_default(),
      height: self.height,
      path: self.path.clone(),
      poster: tail.as_mut().and_then(poster_png),
      primary_kind: match self.source {
        VideoSource::Camera => PrimaryRecordingKind::Camera,
        VideoSource::Screen => PrimaryRecordingKind::Screen,
      },
      // The recording state owns source geometry and fills this before the
      // artifact is presented. The writer itself only deals in pixels.
      source_scale_factor: 1.0,
      width: self.width,
    })
  }
}

pub(super) fn writer_error(writer: &av::AssetWriter, fallback: &str) -> String {
  writer
    .error()
    .map_or_else(|| fallback.to_owned(), |error| error.to_string())
}

/// Draws the still shown in the export window from the recording's last frame.
fn poster_png(frame: &mut Frame) -> Option<Vec<u8>> {
  let buf = &mut *frame.buf;
  // A `420v` capture is always bi-planar; anything else is not a frame this
  // pipeline produced.
  if buf.plane_count() < 2 {
    return None;
  }
  let width = u32::try_from(buf.width()).ok()?;
  let height = u32::try_from(buf.height()).ok()?;
  let (out_width, out_height) = poster_size(width, height, POSTER_MAX_EDGE);

  let flags = cv::pixel_buffer::LockFlags::READ_ONLY;
  // SAFETY: the buffer stays locked for exactly the two reads below, and each
  // plane is bounded by the stride and height the buffer itself reports.
  unsafe { buf.lock_base_addr(flags) }.result().ok()?;
  let luma_stride = buf.plane_bytes_per_row(0);
  let chroma_stride = buf.plane_bytes_per_row(1);
  let luma_base = buf.plane_base_address(0);
  let chroma_base = buf.plane_base_address(1);
  let rgba = if luma_base.is_null() || chroma_base.is_null() {
    None
  } else {
    let luma = Plane {
      bytes: unsafe { std::slice::from_raw_parts(luma_base, luma_stride * buf.plane_height(0)) },
      stride: luma_stride,
    };
    let chroma = Plane {
      bytes: unsafe {
        std::slice::from_raw_parts(chroma_base, chroma_stride * buf.plane_height(1))
      },
      stride: chroma_stride,
    };
    Some(nv12_poster_rgba(
      luma, chroma, width, height, out_width, out_height,
    ))
  };
  unsafe { buf.unlock_lock_base_addr(flags) };

  let image = image::RgbaImage::from_raw(out_width, out_height, rgba?)?;
  let mut png = Vec::new();
  image::DynamicImage::ImageRgba8(image)
    .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
    .ok()?;

  Some(png)
}
