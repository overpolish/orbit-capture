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
fn cursor_metadata_does_not_depend_on_the_native_cursor_pixels() {
  for mode in [
    RecordingMode::Screen,
    RecordingMode::Region,
    RecordingMode::Window,
  ] {
    assert_eq!(
      session::records_cursor(mode),
      cfg!(any(target_os = "macos", target_os = "windows"))
    );
  }
  assert!(!session::records_cursor(RecordingMode::Camera));
  assert!(!session::records_cursor(RecordingMode::Audio));
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

#[test]
fn accepts_a_window_with_a_capture_identifier() {
  let options: StartRecordingOptions =
    serde_json::from_str(r#"{"mode":"window","windowId":42,"fps":30}"#).unwrap();

  assert!(validate_options(&options).is_ok());
  assert_eq!(options.window_id, Some(42));
  assert_eq!(options.fps, 30);
}

#[test]
fn rejects_a_window_without_a_capture_identifier() {
  let options: StartRecordingOptions = serde_json::from_str(r#"{"mode":"window"}"#).unwrap();

  assert_eq!(
    validate_options(&options),
    Err("No window is selected to record".to_owned())
  );
}

fn screen_options() -> StartRecordingOptions {
  StartRecordingOptions {
    mode: RecordingMode::Screen,
    monitor_id: Some(1),
    window_id: None,
    region: None,
    show_cursor: true,
    system_audio: false,
    system_audio_application_ids: Vec::new(),
    system_audio_process_ids: Vec::new(),
    microphone_id: None,
    camera_id: None,
    camera_width: None,
    camera_height: None,
    camera_fps: None,
    camera_flipped: false,
    fps: 60,
  }
}

#[test]
fn vanished_secondary_inputs_are_dropped_not_fatal() {
  let mut options = screen_options();
  options.microphone_id = Some("screenwide-test-missing-microphone".to_owned());
  options.camera_id = Some("screenwide-test-missing-camera".to_owned());
  options.camera_width = Some(1_920);
  options.camera_height = Some(1_080);
  options.camera_fps = Some(30);
  options.camera_flipped = true;
  let skipped = session::drop_unavailable_inputs(&mut options);
  assert_eq!(skipped, ["microphone", "camera"]);
  assert_eq!(options.microphone_id, None);
  assert_eq!(options.camera_id, None);
  assert_eq!(options.camera_width, None);
  assert_eq!(options.camera_height, None);
  assert_eq!(options.camera_fps, None);
  assert!(!options.camera_flipped);
}

#[test]
fn a_camera_recording_keeps_its_vanished_camera_so_the_start_fails_loudly() {
  let mut options = screen_options();
  options.mode = RecordingMode::Camera;
  options.monitor_id = None;
  options.camera_id = Some("screenwide-test-missing-camera".to_owned());
  options.camera_width = Some(1_920);
  options.camera_height = Some(1_080);
  options.camera_fps = Some(30);
  let skipped = session::drop_unavailable_inputs(&mut options);
  assert!(skipped.is_empty());
  assert!(options.camera_id.is_some());
}

#[test]
fn an_audio_recording_keeps_its_sole_vanished_microphone() {
  let mut options = screen_options();
  options.mode = RecordingMode::Audio;
  options.monitor_id = None;
  options.microphone_id = Some("screenwide-test-missing-microphone".to_owned());
  let skipped = session::drop_unavailable_inputs(&mut options);
  assert!(skipped.is_empty());
  assert!(options.microphone_id.is_some());
}

#[test]
fn an_audio_recording_with_system_audio_drops_a_vanished_microphone() {
  let mut options = screen_options();
  options.mode = RecordingMode::Audio;
  options.monitor_id = None;
  options.system_audio = true;
  options.microphone_id = Some("screenwide-test-missing-microphone".to_owned());
  let skipped = session::drop_unavailable_inputs(&mut options);
  assert_eq!(skipped, ["microphone"]);
  assert_eq!(options.microphone_id, None);
}
