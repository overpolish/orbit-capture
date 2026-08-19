// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

//! The parts of the capture pipeline that are arithmetic rather than platform.
//!
//! Everything here is pure so the timing rules - which are what a recording
//! actually is - can be tested without a display, an encoder or a thread.

use std::{path::PathBuf, sync::Arc};

use chrono::NaiveDateTime;
use serde::Serialize;

/// Told once, from the writer thread, when a recording stops being able to
/// accept frames. The user sees one message however many frames follow.
pub type FailureReport = Arc<dyn Fn(String) + Send + Sync>;

/// What a finished recording leaves behind. Platform-independent on purpose:
/// it is what the export window is handed, and the export window knows nothing
/// about how the file was made.
pub struct FinalizeInfo {
  pub camera: Option<CameraFinalizeInfo>,
  pub cursor_path: Option<PathBuf>,
  pub has_microphone: bool,
  pub has_system_audio: bool,
  pub duration_ms: u64,
  pub height: u32,
  pub path: PathBuf,
  pub primary_kind: PrimaryRecordingKind,
  /// The captured pixels per logical display point. Export uses this to offer
  /// meaningful 1x/1.5x output rather than arbitrary percentages.
  pub source_scale_factor: f32,
  pub width: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrimaryRecordingKind {
  Screen,
  Camera,
  Audio,
}

pub struct CameraFinalizeInfo {
  pub duration_ms: u64,
  pub height: u32,
  pub path: PathBuf,
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
    .format(if cfg!(target_os = "windows") {
      "recording-%Y%m%d-%H%M%S%.3f.mp4"
    } else {
      "recording-%Y%m%d-%H%M%S%.3f.mov"
    })
    .to_string()
}

pub fn camera_temp_file_name(started_at: NaiveDateTime) -> String {
  started_at
    .format(if cfg!(target_os = "windows") {
      "camera-%Y%m%d-%H%M%S%.3f.mp4"
    } else {
      "camera-%Y%m%d-%H%M%S%.3f.mov"
    })
    .to_string()
}

pub fn cursor_temp_file_name(started_at: NaiveDateTime) -> String {
  started_at
    .format("recording-%Y%m%d-%H%M%S%.3f.cursor.jsonl")
    .to_string()
}

pub fn audio_temp_file_name(started_at: NaiveDateTime) -> String {
  started_at.format("audio-%Y%m%d-%H%M%S%.3f.mov").to_string()
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
  /// Pins this timeline to a source shared with another writer. Camera and
  /// screen are encoded into separate working movies, but an editor must see
  /// the same zero and the same removed pauses in both.
  pub fn start_at(&mut self, source_ns: i64, wall_ns: i64) {
    self.origin.get_or_insert(Origin { source_ns, wall_ns });
  }

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

  /// Produces a video timestamp from the monotonic wall clock and advances
  /// the same ordering guard used by source-timestamped frames. Windows uses
  /// this for change-driven windows and GPU-cropped regions, keeping their
  /// presentation cadence fixed even when capture delivery is uneven.
  #[cfg(target_os = "windows")]
  pub fn wall_frame_pts_ns(&mut self, wall_ns: i64) -> i64 {
    let mut pts = self.wall_pts_ns(wall_ns);
    if let Some(last) = self.last_pts_ns {
      pts = pts.max(last.saturating_add(ONE_NS));
    }
    self.last_pts_ns = Some(pts);
    pts
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

#[cfg(test)]
mod tests;
