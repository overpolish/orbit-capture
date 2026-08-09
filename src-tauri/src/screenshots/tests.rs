// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use tauri::{LogicalPosition, LogicalSize};

use super::*;

fn region(x: f64, y: f64, width: f64, height: f64) -> Region {
  Region {
    position: LogicalPosition::new(x, y),
    size: LogicalSize::new(width, height),
  }
}

#[test]
fn passes_a_region_through_unchanged_at_one_times_scale() {
  let rect = physical_capture_rect(region(10.0, 20.0, 300.0, 200.0), 1.0, 1920, 1080).unwrap();
  assert_eq!(
    rect,
    CaptureRect {
      x: 10,
      y: 20,
      width: 300,
      height: 200
    }
  );
}

#[test]
fn doubles_a_region_on_a_retina_monitor() {
  let rect = physical_capture_rect(region(10.0, 20.0, 300.0, 200.0), 2.0, 3840, 2160).unwrap();
  assert_eq!(
    rect,
    CaptureRect {
      x: 20,
      y: 40,
      width: 600,
      height: 400
    }
  );
}

#[test]
fn rounds_the_edges_rather_than_the_size() {
  // Rounding the size independently would give 226 here, leaving the right
  // edge a pixel away from where the corner says it is.
  let rect = physical_capture_rect(region(10.4, 0.0, 150.3, 10.0), 1.5, 1920, 1080).unwrap();
  assert_eq!(rect.x, 16);
  assert_eq!(rect.width, 225);
  assert_eq!(rect.x + rect.width, 241);
}

#[test]
fn clamps_a_region_that_runs_past_the_monitor() {
  let rect = physical_capture_rect(region(1800.0, 1000.0, 400.0, 400.0), 1.0, 1920, 1080).unwrap();
  assert_eq!(
    rect,
    CaptureRect {
      x: 1800,
      y: 1000,
      width: 120,
      height: 80
    }
  );
}

#[test]
fn clamps_a_region_that_starts_before_the_monitor() {
  let rect = physical_capture_rect(region(-50.0, -30.0, 200.0, 100.0), 1.0, 1920, 1080).unwrap();
  assert_eq!(
    rect,
    CaptureRect {
      x: 0,
      y: 0,
      width: 150,
      height: 70
    }
  );
}

#[test]
fn fills_the_monitor_exactly_at_its_bounds() {
  let rect = physical_capture_rect(region(0.0, 0.0, 1920.0, 1080.0), 1.0, 1920, 1080).unwrap();
  assert_eq!(rect.width, 1920);
  assert_eq!(rect.height, 1080);
}

#[test]
fn rejects_a_region_entirely_off_the_monitor() {
  assert!(physical_capture_rect(region(2000.0, 0.0, 100.0, 100.0), 1.0, 1920, 1080).is_none());
}

#[test]
fn rejects_an_empty_or_nonsensical_region() {
  assert!(physical_capture_rect(region(0.0, 0.0, 0.0, 100.0), 1.0, 1920, 1080).is_none());
  assert!(physical_capture_rect(region(0.0, 0.0, 100.0, 100.0), 0.0, 1920, 1080).is_none());
  assert!(physical_capture_rect(region(f64::NAN, 0.0, 100.0, 100.0), 1.0, 1920, 1080).is_none());
}

#[test]
fn names_a_still_the_way_the_platform_does() {
  let captured_at = NaiveDate::from_ymd_opt(2026, 8, 8)
    .unwrap()
    .and_hms_opt(14, 32, 5)
    .unwrap();
  assert_eq!(
    capture_file_stem(captured_at),
    "Orbit Capture 2026-08-08 at 14.32.05"
  );
}

#[test]
fn zero_pads_every_field_of_the_name() {
  let captured_at = NaiveDate::from_ymd_opt(2026, 1, 2)
    .unwrap()
    .and_hms_opt(9, 5, 3)
    .unwrap();
  assert_eq!(
    capture_file_stem(captured_at),
    "Orbit Capture 2026-01-02 at 09.05.03"
  );
}

#[test]
fn uses_the_plain_name_when_nothing_is_in_the_way() {
  let path = unique_path(Path::new("/tmp"), "Shot", "png", &|_| false);
  assert_eq!(path, Path::new("/tmp/Shot.png"));
}

#[test]
fn counts_up_past_every_name_already_taken() {
  let taken: HashSet<PathBuf> = ["/tmp/Shot.png", "/tmp/Shot (2).png", "/tmp/Shot (3).png"]
    .iter()
    .map(PathBuf::from)
    .collect();
  let path = unique_path(Path::new("/tmp"), "Shot", "png", &|candidate| {
    taken.contains(candidate)
  });
  assert_eq!(path, Path::new("/tmp/Shot (4).png"));
}

#[test]
fn starts_the_suffix_at_two() {
  let taken: HashSet<PathBuf> = ["/tmp/Shot.png"].iter().map(PathBuf::from).collect();
  let path = unique_path(Path::new("/tmp"), "Shot", "png", &|candidate| {
    taken.contains(candidate)
  });
  assert_eq!(path, Path::new("/tmp/Shot (2).png"));
}

#[test]
fn deserializes_every_target_the_bar_can_send() {
  let screen: ScreenshotTarget =
    serde_json::from_str(r#"{"kind":"screen","monitorId":7}"#).unwrap();
  assert!(matches!(screen, ScreenshotTarget::Screen { monitor_id: 7 }));

  let window: ScreenshotTarget =
    serde_json::from_str(r#"{"kind":"window","windowId":42}"#).unwrap();
  assert!(matches!(window, ScreenshotTarget::Window { window_id: 42 }));

  let region: ScreenshotTarget = serde_json::from_str(
    r#"{"kind":"region","monitorId":7,"region":{"position":{"x":1,"y":2},"size":{"width":3,"height":4}}}"#,
  )
  .unwrap();
  let ScreenshotTarget::Region {
    monitor_id, region, ..
  } = region
  else {
    panic!("expected a region target");
  };
  assert_eq!(monitor_id, 7);
  assert_eq!(region.size.width, 3.0);
}
