//! The parts of the capture pipeline that are arithmetic rather than platform.
//!
//! Everything here is pure so the timing rules - which are what a recording
//! actually is - can be tested without a display, an encoder or a thread.

use std::path::PathBuf;

use chrono::NaiveDateTime;

/// Told once, from the writer thread, when a recording stops being able to
/// accept frames. The user sees one message however many frames follow.
pub type FailureReport = Box<dyn Fn(String) + Send>;

/// What a finished recording leaves behind. Platform-independent on purpose:
/// it is what the export window is handed, and the export window knows nothing
/// about how the file was made.
pub struct FinalizeInfo {
  pub has_microphone: bool,
  pub has_system_audio: bool,
  pub duration_ms: u64,
  pub height: u32,
  pub path: PathBuf,
  /// A still from the last frame, if one could be drawn. A recording recovered
  /// from a previous run has none, because its frames are long gone.
  pub poster: Option<Vec<u8>>,
  /// The captured pixels per logical display point. Export uses this to offer
  /// meaningful 1x/1.5x output rather than arbitrary percentages.
  pub source_scale_factor: f32,
  pub width: u32,
}

/// One nanosecond, in nanoseconds. Named because it is used as a timestamp
/// nudge rather than as a duration.
const ONE_NS: i64 = 1;

/// Bits spent per pixel per frame. The whole bitrate derivation is this
/// constant times the pixel rate, so a 4K 60fps capture and a 720p 30fps one
/// land at proportionate quality instead of sharing one hard-coded number.
const BITS_PER_PIXEL_PER_FRAME: f64 = 0.1;

/// Below this a capture of a small window looks worse than the screen it came
/// from, which is the one thing a screen recorder may not do.
const MIN_BITRATE_BPS: f64 = 1_000_000.0;

/// H.264 hardware encoders top out well below this; the clamp exists so the
/// cast can never wrap, not because the number is reachable.
const MAX_BITRATE_BPS: f64 = 200_000_000.0;

/// The average bitrate to ask the encoder for, in bits per second.
pub fn bitrate_bps(width: u32, height: u32, fps: u32) -> i32 {
  let pixels_per_second = f64::from(width) * f64::from(height) * f64::from(fps);
  let bitrate = (pixels_per_second * BITS_PER_PIXEL_PER_FRAME).round();

  bitrate.clamp(MIN_BITRATE_BPS, MAX_BITRATE_BPS) as i32
}

/// The name of the working file a recording is written to while it runs. Sorts
/// chronologically, and carries milliseconds so two recordings started in the
/// same second cannot collide.
///
/// A QuickTime movie, not an .mp4, because only QuickTime survives being
/// written in fragments and only a fragmented file is worth anything if the
/// app dies mid-recording. The saved file is still an .mp4 - the working movie
/// is stream-copied into one when the user keeps it. See
/// `platform::Container::quicktime_fragmented` and `exports::save_recording`.
pub fn temp_file_name(started_at: NaiveDateTime) -> String {
  started_at
    .format("recording-%Y%m%d-%H%M%S%.3f.mov")
    .to_string()
}

#[derive(Clone, Copy, Debug)]
struct Origin {
  source_ns: i64,
  wall_ns: i64,
}

/// The mapping from captured frames onto the movie's own timeline.
///
/// Two clocks are involved and both are needed. Frame timestamps come from
/// ScreenCaptureKit and are what keeps motion smooth, so appended frames are
/// rebased off the first frame's timestamp. The stop timestamp cannot come
/// from that clock at all: ScreenCaptureKit stops sending frames when nothing
/// on screen changes, so on a static screen the last frame may be minutes old
/// and ending the movie there would truncate it. The stop timestamp is
/// therefore derived from a monotonic wall reading anchored to the same first
/// frame. Both clocks tick at the same rate, so anchoring them together is
/// what lets the two be mixed.
///
/// Paused time is subtracted from everything downstream of it, which is what
/// makes a paused recording play back as though the pause never happened.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeline {
  origin: Option<Origin>,
  last_pts_ns: Option<i64>,
  paused_since_ns: Option<i64>,
  paused_total_ns: i64,
}

impl Timeline {
  /// Whether a first frame has been appended, which is what starts the movie.
  pub const fn has_started(&self) -> bool {
    self.origin.is_some()
  }

  pub const fn is_paused(&self) -> bool {
    self.paused_since_ns.is_some()
  }

  /// Paused time so far, counting an open pause up to `wall_ns`.
  fn paused_total_at(&self, wall_ns: i64) -> i64 {
    match self.paused_since_ns {
      Some(since) => self
        .paused_total_ns
        .saturating_add(wall_ns.saturating_sub(since).max(0)),
      None => self.paused_total_ns,
    }
  }

  /// Opens a pause. Pausing an already paused timeline is ignored rather than
  /// treated as an error: the state machine rejects that transition, and if
  /// one ever slipped through, dropping it is what keeps the clock honest.
  pub fn pause(&mut self, wall_ns: i64) {
    if self.paused_since_ns.is_none() {
      self.paused_since_ns = Some(wall_ns);
    }
  }

  /// Closes a pause, folding its span into the running total.
  pub fn resume(&mut self, wall_ns: i64) {
    if let Some(since) = self.paused_since_ns.take() {
      self.paused_total_ns = self
        .paused_total_ns
        .saturating_add(wall_ns.saturating_sub(since).max(0));
    }
  }

  /// The timestamp a frame should be written at, adopting the first frame as
  /// the movie's origin.
  ///
  /// The result is forced to keep increasing: the writer rejects a frame whose
  /// timestamp does not advance, and two frames can land on the same
  /// nanosecond when the pause bookkeeping pulls them together.
  pub fn media_pts_ns(&mut self, source_ns: i64, wall_ns: i64) -> i64 {
    let origin = *self.origin.get_or_insert(Origin { source_ns, wall_ns });
    let elapsed = source_ns.saturating_sub(origin.source_ns);
    elapsed.saturating_sub(self.paused_total_at(wall_ns)).max(0)
  }

  pub fn frame_pts_ns(&mut self, source_ns: i64, wall_ns: i64) -> i64 {
    let mut pts = self.media_pts_ns(source_ns, wall_ns);
    if let Some(last) = self.last_pts_ns {
      pts = pts.max(last.saturating_add(ONE_NS));
    }
    self.last_pts_ns = Some(pts);

    pts
  }

  /// Maps media whose source clock is unrelated to ScreenCaptureKit. CPAL's
  /// capture timestamp is translated onto this monotonic wall clock before it
  /// reaches the timeline, so microphone latency is measured rather than
  /// compensated with a device-specific constant.
  pub fn wall_pts_ns(&self, wall_ns: i64) -> i64 {
    let Some(origin) = self.origin else {
      return 0;
    };
    wall_ns
      .saturating_sub(origin.wall_ns)
      .saturating_sub(self.paused_total_at(wall_ns))
      .max(0)
  }

  /// Where the movie ends, in its own timeline. This is also the point the
  /// cached final frame is appended at, so a static screen still produces a
  /// movie as long as the user watched it.
  pub fn stop_pts_ns(&self, wall_ns: i64) -> i64 {
    let Some(origin) = self.origin else {
      return 0;
    };
    let elapsed = wall_ns.saturating_sub(origin.wall_ns);
    let after_last = self
      .last_pts_ns
      .map_or(0, |last| last.saturating_add(ONE_NS));

    elapsed
      .saturating_sub(self.paused_total_at(wall_ns))
      .max(0)
      .max(after_last)
  }
}

/// Scales a capture down to fit `max_edge`, never up.
pub fn poster_size(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
  let longest = width.max(height);
  if longest == 0 {
    return (0, 0);
  }
  if longest <= max_edge {
    return (width, height);
  }

  let scale = f64::from(max_edge) / f64::from(longest);
  (
    ((f64::from(width) * scale).round() as u32).max(1),
    ((f64::from(height) * scale).round() as u32).max(1),
  )
}

/// BT.709 video-range coefficients, which is what ScreenCaptureKit hands back
/// for a `420v` capture.
const LUMA_SCALE: f32 = 1.164_383_5;
const R_V: f32 = 1.792_741_1;
const G_U: f32 = -0.213_248_6;
const G_V: f32 = -0.532_909_3;
const B_U: f32 = 2.112_401_8;

/// One plane of a captured frame. The stride is the buffer's own row pitch,
/// which for a hardware capture is padded well past the image's width.
pub struct Plane<'a> {
  pub bytes: &'a [u8],
  pub stride: usize,
}

/// Converts a bi-planar NV12 frame straight into a downscaled RGBA thumbnail.
///
/// Sampling on the way down rather than converting the whole frame first is
/// what keeps this cheap: a 4K frame costs a few hundred thousand pixel
/// conversions instead of eight million. This runs once per recording, on the
/// writer thread, so nearest-neighbour sampling is the right trade - the
/// result is a poster image, not a frame of the movie.
pub fn nv12_poster_rgba(
  luma: Plane<'_>,
  chroma: Plane<'_>,
  width: u32,
  height: u32,
  out_width: u32,
  out_height: u32,
) -> Vec<u8> {
  let (
    Plane {
      bytes: luma,
      stride: luma_stride,
    },
    Plane {
      bytes: chroma,
      stride: chroma_stride,
    },
  ) = (luma, chroma);
  let mut rgba = vec![0_u8; (out_width as usize) * (out_height as usize) * 4];
  if out_width == 0 || out_height == 0 || width == 0 || height == 0 {
    return rgba;
  }

  for out_y in 0..out_height as usize {
    let source_y = (out_y * height as usize / out_height as usize).min(height as usize - 1);
    let luma_row = source_y * luma_stride;
    let chroma_row = (source_y / 2) * chroma_stride;

    for out_x in 0..out_width as usize {
      let source_x = (out_x * width as usize / out_width as usize).min(width as usize - 1);
      let y = luma.get(luma_row + source_x).copied().unwrap_or(16);
      let u = chroma
        .get(chroma_row + (source_x / 2) * 2)
        .copied()
        .unwrap_or(128);
      let v = chroma
        .get(chroma_row + (source_x / 2) * 2 + 1)
        .copied()
        .unwrap_or(128);

      let luminance = LUMA_SCALE * (f32::from(y) - 16.0);
      let chroma_u = f32::from(u) - 128.0;
      let chroma_v = f32::from(v) - 128.0;
      let pixel = (out_y * out_width as usize + out_x) * 4;
      rgba[pixel] = clamp_channel(luminance + R_V * chroma_v);
      rgba[pixel + 1] = clamp_channel(luminance + G_U * chroma_u + G_V * chroma_v);
      rgba[pixel + 2] = clamp_channel(luminance + B_U * chroma_u);
      rgba[pixel + 3] = u8::MAX;
    }
  }

  rgba
}

fn clamp_channel(value: f32) -> u8 {
  value.clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
  use chrono::NaiveDate;

  use super::*;

  const MS: i64 = 1_000_000;
  /// An arbitrary non-zero epoch for each clock, so nothing can pass by
  /// accidentally treating a raw timestamp as an elapsed one.
  const SOURCE_EPOCH: i64 = 900_000 * MS;
  const WALL_EPOCH: i64 = 55_000 * MS;

  /// Frames arrive on both clocks at once; only the epochs differ.
  fn frame(timeline: &mut Timeline, at_ms: i64) -> i64 {
    timeline.frame_pts_ns(SOURCE_EPOCH + at_ms * MS, WALL_EPOCH + at_ms * MS)
  }

  #[test]
  fn starts_the_movie_at_zero_whatever_the_capture_clock_reads() {
    let mut timeline = Timeline::default();
    assert!(!timeline.has_started());

    assert_eq!(frame(&mut timeline, 0), 0);
    assert!(timeline.has_started());
  }

  #[test]
  fn keeps_the_spacing_between_frames() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);

    assert_eq!(frame(&mut timeline, 16), 16 * MS);
    assert_eq!(frame(&mut timeline, 33), 33 * MS);
  }

  #[test]
  fn takes_a_pause_out_of_every_timestamp_after_it() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    frame(&mut timeline, 1_000);

    timeline.pause(WALL_EPOCH + 2_000 * MS);
    assert!(timeline.is_paused());
    timeline.resume(WALL_EPOCH + 7_000 * MS);
    assert!(!timeline.is_paused());

    // Five seconds of wall time passed but only three seconds were recorded.
    assert_eq!(frame(&mut timeline, 8_000), 3_000 * MS);
  }

  #[test]
  fn maps_an_independent_media_clock_through_wall_time() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);

    assert_eq!(timeline.wall_pts_ns(WALL_EPOCH + 725 * MS), 725 * MS);
  }

  #[test]
  fn removes_pauses_from_independent_media_too() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    timeline.pause(WALL_EPOCH + 500 * MS);
    timeline.resume(WALL_EPOCH + 2_500 * MS);

    assert_eq!(timeline.wall_pts_ns(WALL_EPOCH + 3_000 * MS), 1_000 * MS);
  }

  #[test]
  fn accumulates_across_several_pauses() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);

    timeline.pause(WALL_EPOCH + 1_000 * MS);
    timeline.resume(WALL_EPOCH + 3_000 * MS);
    timeline.pause(WALL_EPOCH + 4_000 * MS);
    timeline.resume(WALL_EPOCH + 10_000 * MS);
    timeline.pause(WALL_EPOCH + 11_000 * MS);
    timeline.resume(WALL_EPOCH + 11_500 * MS);

    // 2000 + 6000 + 500 paused out of 12 seconds elapsed.
    assert_eq!(frame(&mut timeline, 12_000), 3_500 * MS);
  }

  #[test]
  fn ends_where_the_wall_clock_says_even_with_no_recent_frame() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    frame(&mut timeline, 200);

    // Nothing changed on screen for a minute, so no frames arrived - the
    // movie still has to be a minute long.
    assert_eq!(timeline.stop_pts_ns(WALL_EPOCH + 60_000 * MS), 60_000 * MS);
  }

  #[test]
  fn counts_an_open_pause_against_the_ending() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    timeline.pause(WALL_EPOCH + 2_000 * MS);

    // Stopped while paused after two seconds of recording and eight of pause.
    assert_eq!(timeline.stop_pts_ns(WALL_EPOCH + 10_000 * MS), 2_000 * MS);
  }

  /// The exact shape that made a real recording fail: the first frame after a
  /// resume must land strictly after the last one that was written, and at the
  /// time the recording has actually been running for.
  #[test]
  fn resuming_lands_the_next_frame_after_the_last_one_written() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    let last_written = frame(&mut timeline, 1_000);

    timeline.pause(WALL_EPOCH + 1_016 * MS);
    // Frames keep arriving through the pause; none of them are written, so
    // none of them may move the clock on.
    timeline.resume(WALL_EPOCH + 31_016 * MS);

    let resumed = frame(&mut timeline, 31_032);
    assert!(
      resumed > last_written,
      "{resumed} must come after {last_written}"
    );
    // Thirty seconds of pause out of 31.032 seconds elapsed leaves 1.032.
    assert_eq!(resumed, 1_032 * MS);
    // And it must not have been forced there by the monotonic guard, which
    // would mean the real arithmetic had gone backwards.
    assert!(resumed > last_written + ONE_NS);
  }

  #[test]
  fn keeps_climbing_across_a_pause_frame_after_frame() {
    let mut timeline = Timeline::default();
    let mut previous = frame(&mut timeline, 0);
    for tick in 1..60 {
      let pts = frame(&mut timeline, tick * 16);
      assert!(pts > previous, "frame {tick} went backwards");
      previous = pts;
    }

    timeline.pause(WALL_EPOCH + 944 * MS);
    timeline.resume(WALL_EPOCH + 10_944 * MS);

    for tick in 60..120 {
      let pts = frame(&mut timeline, 10_000 + tick * 16);
      assert!(
        pts > previous,
        "frame {tick} went backwards after the resume"
      );
      previous = pts;
    }
    // Ten seconds of pause taken out of a 11.9 second run.
    assert_eq!(previous, 1_904 * MS);
  }

  #[test]
  fn never_lets_a_timestamp_stand_still() {
    let mut timeline = Timeline::default();
    frame(&mut timeline, 0);
    let first = frame(&mut timeline, 500);
    // The same instant twice, which the writer would reject outright.
    let second = frame(&mut timeline, 500);

    assert_eq!(second, first + 1);
    assert!(timeline.stop_pts_ns(WALL_EPOCH + 500 * MS) > second);
  }

  #[test]
  fn has_no_ending_before_it_has_a_beginning() {
    let timeline = Timeline::default();
    assert_eq!(timeline.stop_pts_ns(WALL_EPOCH), 0);
  }

  #[test]
  fn scales_the_bitrate_with_the_pixel_rate() {
    assert_eq!(bitrate_bps(1920, 1080, 30), 6_220_800);
    assert_eq!(bitrate_bps(1920, 1080, 60), 12_441_600);
    assert_eq!(bitrate_bps(3840, 2160, 60), 49_766_400);
  }

  #[test]
  fn keeps_a_tiny_capture_watchable() {
    assert_eq!(bitrate_bps(320, 200, 30), MIN_BITRATE_BPS as i32);
  }

  #[test]
  fn names_the_working_file_so_it_sorts_by_time() {
    let started_at = NaiveDate::from_ymd_opt(2026, 8, 8)
      .unwrap()
      .and_hms_milli_opt(14, 32, 5, 250)
      .unwrap();
    assert_eq!(
      temp_file_name(started_at),
      "recording-20260808-143205.250.mov"
    );
  }

  #[test]
  fn fits_a_poster_inside_its_longest_edge() {
    assert_eq!(poster_size(3840, 2160, 640), (640, 360));
    assert_eq!(poster_size(1080, 1920, 640), (360, 640));
  }

  #[test]
  fn leaves_a_poster_smaller_than_the_limit_alone() {
    assert_eq!(poster_size(400, 300, 640), (400, 300));
    assert_eq!(poster_size(0, 0, 640), (0, 0));
  }

  /// A flat NV12 frame, with the strides padded the way a capture buffer is.
  fn flat_nv12(width: usize, height: usize, y: u8, u: u8, v: u8) -> (Vec<u8>, Vec<u8>) {
    let stride = width + 64;
    let luma = vec![y; stride * height];
    let mut chroma = vec![0_u8; stride * height.div_ceil(2)];
    for pair in chroma.chunks_exact_mut(2) {
      pair[0] = u;
      pair[1] = v;
    }

    (luma, chroma)
  }

  /// The padded stride `flat_nv12` builds its planes with.
  fn stride(width: usize) -> usize {
    width + 64
  }

  #[test]
  fn turns_flat_luma_into_flat_grey() {
    let (luma, chroma) = flat_nv12(64, 64, 126, 128, 128);
    let rgba = nv12_poster_rgba(
      Plane {
        bytes: &luma,
        stride: stride(64),
      },
      Plane {
        bytes: &chroma,
        stride: stride(64),
      },
      64,
      64,
      16,
      16,
    );

    assert_eq!(rgba.len(), 16 * 16 * 4);
    for pixel in rgba.chunks_exact(4) {
      assert!(pixel[0].abs_diff(128) <= 2, "unexpected red {}", pixel[0]);
      assert_eq!(pixel[0], pixel[1]);
      assert_eq!(pixel[1], pixel[2]);
      assert_eq!(pixel[3], u8::MAX);
    }
  }

  #[test]
  fn carries_chroma_through_to_colour() {
    // Video-range white and a strong red chroma.
    let (luma, chroma) = flat_nv12(32, 32, 81, 90, 240);
    let rgba = nv12_poster_rgba(
      Plane {
        bytes: &luma,
        stride: stride(32),
      },
      Plane {
        bytes: &chroma,
        stride: stride(32),
      },
      32,
      32,
      8,
      8,
    );

    let pixel = &rgba[..4];
    assert!(pixel[0] > 200, "red should dominate, got {pixel:?}");
    assert!(pixel[1] < 80, "green should be low, got {pixel:?}");
    assert!(pixel[2] < 80, "blue should be low, got {pixel:?}");
  }

  #[test]
  fn survives_a_frame_whose_planes_are_shorter_than_claimed() {
    let rgba = nv12_poster_rgba(
      Plane {
        bytes: &[],
        stride: 0,
      },
      Plane {
        bytes: &[],
        stride: 0,
      },
      32,
      32,
      4,
      4,
    );
    assert_eq!(rgba.len(), 4 * 4 * 4);
  }
}
