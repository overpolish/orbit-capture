// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const DOCK: PhysicalSize<u32> = PhysicalSize {
  width: 198,
  height: 44,
};
const WORK_AREA: PhysicalSize<u32> = PhysicalSize {
  width: 1440,
  height: 850,
};

#[test]
fn centres_the_pill_below_the_work_area_top_when_it_was_never_dragged() {
  let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, None);
  assert_eq!(x, (1440 - 198) / 2);
  assert_eq!(y, RECORDING_DOCK_TOP_GAP as i32);
}

#[test]
fn scales_the_default_gap_with_the_monitor() {
  let work_area = PhysicalSize {
    width: 2880,
    height: 1700,
  };
  let dock = PhysicalSize {
    width: 432,
    height: 88,
  };
  let (x, y) = recording_dock_local_position(work_area, dock, 2.0, None);
  assert_eq!(x, (2880 - 432) / 2);
  assert_eq!(y, (RECORDING_DOCK_TOP_GAP * 2.0) as i32);
}

#[test]
fn applies_a_saved_offset_relative_to_the_work_area() {
  let offset = Some(RecordingDockOffset { x: 200.0, y: 60.0 });
  let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, offset);
  assert_eq!((x, y), (200, 60));
}

#[test]
fn keeps_a_saved_offset_the_same_visual_distance_on_a_retina_monitor() {
  let offset = Some(RecordingDockOffset { x: 200.0, y: 60.0 });
  let work_area = PhysicalSize {
    width: 2880,
    height: 1700,
  };
  let dock = PhysicalSize {
    width: 432,
    height: 88,
  };
  let (x, y) = recording_dock_local_position(work_area, dock, 2.0, offset);
  assert_eq!((x, y), (400, 120));
}

#[test]
fn clamps_a_saved_offset_onto_a_smaller_monitor() {
  let offset = Some(RecordingDockOffset {
    x: 2_000.0,
    y: 1_400.0,
  });
  let work_area = PhysicalSize {
    width: 1280,
    height: 700,
  };
  let (x, y) = recording_dock_local_position(work_area, DOCK, 1.0, offset);
  assert_eq!((x, y), (1280 - 198, 700 - 44));
}

#[test]
fn clamps_a_negative_offset_back_inside_the_work_area() {
  let offset = Some(RecordingDockOffset { x: -50.0, y: -80.0 });
  let (x, y) = recording_dock_local_position(WORK_AREA, DOCK, 1.0, offset);
  assert_eq!((x, y), (0, 0));
}

#[test]
fn keeps_a_pill_wider_than_its_work_area_at_the_origin() {
  let work_area = PhysicalSize {
    width: 100,
    height: 20,
  };
  let (x, y) = recording_dock_local_position(work_area, DOCK, 1.0, None);
  assert_eq!((x, y), (0, 0));
}
