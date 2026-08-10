// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const START: u64 = 1_000_000;

fn starting() -> RecordingSnapshot {
  let mut snapshot = RecordingSnapshot::default();
  apply_transition(
    &mut snapshot,
    RecordingStatus::Starting,
    Some(RecordingMode::Screen),
    START,
  )
  .unwrap();
  snapshot
}

fn recording() -> RecordingSnapshot {
  let mut snapshot = starting();
  apply_transition(&mut snapshot, RecordingStatus::Recording, None, START).unwrap();
  snapshot
}

#[test]
fn accepts_every_legal_transition() {
  let legal = [
    (RecordingStatus::Idle, RecordingStatus::Starting),
    (RecordingStatus::Starting, RecordingStatus::Recording),
    (RecordingStatus::Starting, RecordingStatus::Idle),
    (RecordingStatus::Recording, RecordingStatus::Paused),
    (RecordingStatus::Paused, RecordingStatus::Recording),
    (RecordingStatus::Recording, RecordingStatus::Stopping),
    (RecordingStatus::Paused, RecordingStatus::Stopping),
    (RecordingStatus::Stopping, RecordingStatus::Idle),
  ];

  for (from, to) in legal {
    let mut snapshot = RecordingSnapshot {
      status: from,
      ..RecordingSnapshot::default()
    };
    assert!(
      apply_transition(&mut snapshot, to, None, START).is_ok(),
      "{} to {} should be legal",
      from.label(),
      to.label()
    );
    assert_eq!(snapshot.status, to);
  }
}

#[test]
fn rejects_every_illegal_transition() {
  let all = [
    RecordingStatus::Idle,
    RecordingStatus::Starting,
    RecordingStatus::Recording,
    RecordingStatus::Paused,
    RecordingStatus::Stopping,
  ];
  let legal = [
    (RecordingStatus::Idle, RecordingStatus::Starting),
    (RecordingStatus::Starting, RecordingStatus::Recording),
    (RecordingStatus::Starting, RecordingStatus::Idle),
    (RecordingStatus::Recording, RecordingStatus::Paused),
    (RecordingStatus::Paused, RecordingStatus::Recording),
    (RecordingStatus::Recording, RecordingStatus::Stopping),
    (RecordingStatus::Paused, RecordingStatus::Stopping),
    (RecordingStatus::Stopping, RecordingStatus::Idle),
  ];

  for from in all {
    for to in all {
      if legal.contains(&(from, to)) {
        continue;
      }

      let mut snapshot = RecordingSnapshot {
        status: from,
        ..RecordingSnapshot::default()
      };
      let error = apply_transition(&mut snapshot, to, None, START).unwrap_err();
      assert!(error.contains(from.label()) && error.contains(to.label()));
      assert_eq!(
        snapshot.status, from,
        "a rejected transition must not mutate the snapshot"
      );
    }
  }
}

#[test]
fn rejects_a_second_start_while_starting() {
  let mut snapshot = starting();
  assert!(apply_transition(
    &mut snapshot,
    RecordingStatus::Starting,
    Some(RecordingMode::Camera),
    START
  )
  .is_err());
  assert_eq!(snapshot.mode, Some(RecordingMode::Screen));
}

#[test]
fn starts_the_clock_when_recording_begins() {
  let snapshot = recording();
  assert_eq!(snapshot.started_at_ms, Some(START));
  assert_eq!(snapshot.accumulated_ms, 0);
  assert_eq!(snapshot.paused_at_ms, None);
}

#[test]
fn folds_the_open_span_into_accumulated_time_on_pause() {
  let mut snapshot = recording();
  apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 5_000).unwrap();

  assert_eq!(snapshot.accumulated_ms, 5_000);
  assert_eq!(snapshot.paused_at_ms, Some(START + 5_000));
  assert_eq!(snapshot.started_at_ms, None);
}

#[test]
fn resuming_restarts_the_span_without_counting_the_pause() {
  let mut snapshot = recording();
  apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 5_000).unwrap();
  apply_transition(
    &mut snapshot,
    RecordingStatus::Recording,
    None,
    START + 25_000,
  )
  .unwrap();

  assert_eq!(snapshot.accumulated_ms, 5_000);
  assert_eq!(snapshot.started_at_ms, Some(START + 25_000));
  assert_eq!(snapshot.paused_at_ms, None);

  apply_transition(
    &mut snapshot,
    RecordingStatus::Stopping,
    None,
    START + 28_000,
  )
  .unwrap();
  assert_eq!(snapshot.accumulated_ms, 8_000);
}

#[test]
fn stopping_from_paused_keeps_the_frozen_elapsed_time() {
  let mut snapshot = recording();
  apply_transition(&mut snapshot, RecordingStatus::Paused, None, START + 3_000).unwrap();
  apply_transition(
    &mut snapshot,
    RecordingStatus::Stopping,
    None,
    START + 90_000,
  )
  .unwrap();

  assert_eq!(snapshot.accumulated_ms, 3_000);
}

#[test]
fn returning_to_idle_clears_the_snapshot() {
  let mut snapshot = recording();
  apply_transition(
    &mut snapshot,
    RecordingStatus::Stopping,
    None,
    START + 1_000,
  )
  .unwrap();
  apply_transition(&mut snapshot, RecordingStatus::Idle, None, START + 1_250).unwrap();

  assert_eq!(snapshot, RecordingSnapshot::default());
}

#[test]
fn defaults_the_frame_rate_when_an_older_bar_omits_it() {
  let options: StartRecordingOptions =
    serde_json::from_str(r#"{"mode":"screen","monitorId":7}"#).unwrap();

  assert_eq!(options.fps, DEFAULT_FPS);
  assert_eq!(options.monitor_id, Some(7));
}

#[test]
fn takes_the_frame_rate_the_bar_sends() {
  let options: StartRecordingOptions =
    serde_json::from_str(r#"{"mode":"screen","monitorId":7,"fps":30}"#).unwrap();

  assert_eq!(options.fps, 30);
}

#[test]
fn accepts_a_region_with_monitor_local_geometry() {
  let options: StartRecordingOptions = serde_json::from_str(
    r#"{
      "mode":"region",
      "monitorId":7,
      "region":{
        "position":{"x":100,"y":50},
        "size":{"width":1280,"height":720}
      }
    }"#,
  )
  .unwrap();

  assert!(validate_options(&options).is_ok());
  assert_eq!(options.region.unwrap().position.x, 100.0);
}

#[test]
fn rejects_a_region_without_geometry() {
  let options: StartRecordingOptions =
    serde_json::from_str(r#"{"mode":"region","monitorId":7}"#).unwrap();

  assert_eq!(
    validate_options(&options),
    Err("No region is selected to record".to_owned())
  );
}
