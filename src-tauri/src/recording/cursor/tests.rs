// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn test_event(at: Instant, kind: RawCursorEventKind) -> RawCursorEvent {
  RawCursorEvent {
    appearance: CursorAppearance {
      height: 24.0,
      hotspot_x: 2.0,
      hotspot_y: 3.0,
      style: CursorStyle::Arrow,
      width: 16.0,
    },
    at,
    kind,
    x: 12.0,
    y: 34.0,
  }
}

#[test]
fn clock_removes_paused_time() {
  let origin = Instant::now();
  let shared_origin = Arc::new(OnceLock::new());
  shared_origin.set(origin).unwrap();
  let mut clock = CursorClock::new(shared_origin);

  assert_eq!(
    clock.timestamp_us(origin + Duration::from_secs(2)),
    Some(2_000_000)
  );
  clock.pause(origin + Duration::from_secs(3));
  assert_eq!(clock.timestamp_us(origin + Duration::from_secs(5)), None);
  clock.resume(origin + Duration::from_secs(8));
  assert_eq!(
    clock.timestamp_us(origin + Duration::from_secs(9)),
    Some(4_000_000)
  );
}

#[test]
fn reader_keeps_complete_lines_before_a_truncated_tail() {
  let path = std::env::temp_dir().join(format!(
    "screenwide-cursor-reader-{}.jsonl",
    std::process::id()
  ));
  std::fs::write(
    &path,
    concat!(
      "{\"type\":\"header\",\"coordinateSpace\":\"global-logical-points\",",
      "\"platform\":\"test\",\"source\":{\"height\":100.0,\"kind\":\"screen\",",
      "\"platformId\":\"1\",\"videoHeight\":200,\"videoWidth\":200,",
      "\"width\":100.0,\"x\":0.0,\"y\":0.0},",
      "\"timebase\":\"recording-microseconds\",\"version\":1}\n",
      "{\"type\":\"position\",\"timestampUs\":10,\"x\":2.0,\"y\":3.0}\n",
      "{\"type\":\"position\""
    ),
  )
  .unwrap();

  let records = read(&path).unwrap();
  let _ = std::fs::remove_file(path);
  assert_eq!(records.len(), 2);
  assert!(matches!(
    records[1],
    CursorRecord::Position {
      timestamp_us: 10,
      ..
    }
  ));
}

#[test]
fn initial_snapshot_starts_at_zero_and_motion_keeps_hardware_cadence() {
  let origin = Instant::now();
  let shared_origin = Arc::new(OnceLock::new());
  shared_origin.set(origin).unwrap();
  let path = std::env::temp_dir().join(format!(
    "screenwide-cursor-stream-{}.jsonl",
    std::process::id()
  ));
  let file = File::create(&path).unwrap();
  let mut stream = StreamWriter {
    clock: CursorClock::new(shared_origin),
    failure: None,
    last_appearance: None,
    last_flush: origin,
    last_move: None,
    writer: BufWriter::new(file),
  };

  stream
    .record(test_event(
      origin + Duration::from_millis(50),
      RawCursorEventKind::Snapshot,
    ))
    .unwrap();
  stream
    .record(test_event(
      origin + Duration::from_millis(54),
      RawCursorEventKind::Move,
    ))
    .unwrap();
  stream
    .record(test_event(
      origin + Duration::from_millis(58),
      RawCursorEventKind::Move,
    ))
    .unwrap();
  stream.writer.flush().unwrap();
  drop(stream);

  let records = std::fs::read_to_string(&path)
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str::<CursorRecord>(line).unwrap())
    .collect::<Vec<_>>();
  let _ = std::fs::remove_file(path);
  assert!(matches!(
    records.as_slice(),
    [
      CursorRecord::Appearance {
        timestamp_us: 0,
        ..
      },
      CursorRecord::Position {
        timestamp_us: 0,
        ..
      },
      CursorRecord::Position {
        timestamp_us: 58_000,
        ..
      }
    ]
  ));
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an interactive Windows desktop"]
fn windows_sampler_writes_a_live_semantic_snapshot() {
  let origin = Arc::new(OnceLock::new());
  origin.set(Instant::now()).unwrap();
  let path = std::env::temp_dir().join(format!(
    "screenwide-windows-cursor-live-{}.jsonl",
    std::process::id()
  ));
  let recorder = CursorRecorder::start(
    path,
    origin,
    CursorSource {
      height: 1_080.0,
      kind: CursorSourceKind::Screen,
      platform_id: "test".to_owned(),
      video_height: 1_080,
      video_width: 1_920,
      width: 1_920.0,
      x: 0.0,
      y: 0.0,
    },
  )
  .unwrap();
  std::thread::sleep(Duration::from_millis(40));
  let path = recorder.stop().unwrap();
  let records = read(&path).unwrap();
  let _ = std::fs::remove_file(path);
  assert!(matches!(
    records.first(),
    Some(CursorRecord::Header {
      coordinate_space,
      platform,
      ..
    }) if coordinate_space == "global-physical-pixels" && platform == "windows"
  ));
  assert!(records
    .iter()
    .any(|record| matches!(record, CursorRecord::Appearance { .. })));
  assert!(records
    .iter()
    .any(|record| matches!(record, CursorRecord::Position { .. })));
}
