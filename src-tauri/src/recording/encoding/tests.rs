// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

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
fn a_secondary_video_keeps_the_primary_videos_zero() {
  let mut timeline = Timeline::default();
  timeline.start_at(WALL_EPOCH, WALL_EPOCH);

  assert_eq!(
    timeline.frame_pts_ns(WALL_EPOCH + 125 * MS, WALL_EPOCH + 125 * MS),
    125 * MS
  );
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
    if cfg!(target_os = "windows") {
      "recording-20260808-143205.250.mp4"
    } else {
      "recording-20260808-143205.250.mov"
    }
  );
}

#[test]
fn names_audio_working_files_separately_from_movies() {
  let started = chrono::NaiveDate::from_ymd_opt(2026, 8, 10)
    .unwrap()
    .and_hms_milli_opt(12, 34, 56, 789)
    .unwrap();

  assert_eq!(
    audio_temp_file_name(started),
    "audio-20260810-123456.789.mov"
  );
}
