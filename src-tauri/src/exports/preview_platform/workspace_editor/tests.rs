// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn target(id: u64, rect: DisplayRect, z_order: i32, selected: bool) -> DisplayTarget {
  DisplayTarget {
    id,
    rect,
    z_order,
    selected: u8::from(selected),
    visible: 1,
    radius_enabled: 1,
    radius_percent: 20.0,
  }
}

#[test]
fn inactive_target_edge_only_selects_its_body() {
  let targets = [target(
    1,
    DisplayRect {
      x: 50.0,
      y: 0.0,
      width: 50.0,
      height: 100.0,
    },
    0,
    false,
  )];
  let hit = hit_test_display(&targets, (50.0, 50.0), 8.0).unwrap();
  assert_eq!((hit.target_id, hit.handle), (1, DisplayHandle::Body as u8));
}

#[test]
fn overlapping_body_picks_top_layer() {
  let targets = [
    target(
      1,
      DisplayRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
      },
      1,
      false,
    ),
    target(
      2,
      DisplayRect {
        x: 20.0,
        y: 20.0,
        width: 80.0,
        height: 80.0,
      },
      2,
      false,
    ),
  ];
  assert_eq!(
    hit_test_display(&targets, (50.0, 50.0), 4.0)
      .unwrap()
      .target_id,
    2
  );
}

#[test]
fn selected_resize_handle_wins_over_neighbouring_target() {
  let targets = [
    target(
      1,
      DisplayRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
      },
      0,
      true,
    ),
    target(
      2,
      DisplayRect {
        x: 92.0,
        y: 20.0,
        width: 80.0,
        height: 80.0,
      },
      1,
      false,
    ),
  ];
  // The selected target's east handle overlaps both target 2's body and its
  // invisible west-handle region. The visible selected handle must win.
  let hit = hit_test_display(&targets, (99.0, 57.0), 8.0).unwrap();
  assert_eq!((hit.target_id, hit.handle), (1, DisplayHandle::East as u8));
}

#[test]
fn selected_radius_point_precedes_other_handles() {
  let targets = [target(
    7,
    DisplayRect {
      x: 0.0,
      y: 0.0,
      width: 100.0,
      height: 100.0,
    },
    0,
    true,
  )];
  let hit = hit_test_display(&targets, (21.0, 21.0), 8.0).unwrap();
  assert_eq!(hit.handle, DisplayHandle::Radius as u8);
}

#[test]
fn disabled_video_radius_falls_back_to_body_or_resize() {
  let mut video = target(
    3,
    DisplayRect {
      x: 0.0,
      y: 0.0,
      width: 100.0,
      height: 100.0,
    },
    0,
    true,
  );
  video.radius_enabled = 0;
  let hit = hit_test_display(&[video], (21.0, 21.0), 8.0).unwrap();
  assert_eq!(hit.handle, DisplayHandle::Body as u8);
}
fn r(x: f64, y: f64, width: f64, height: f64) -> WorldRect {
  WorldRect {
    x,
    y,
    width,
    height,
  }
}
fn n(x: f64, y: f64, width: f64, height: f64) -> NormalizedRect {
  NormalizedRect {
    x,
    y,
    width,
    height,
  }
}
fn geometry(crop: NormalizedRect) -> LayerGeometry {
  LayerGeometry {
    crop,
    image_center_x: crop.x + crop.width / 2.0,
    image_center_y: crop.y + crop.height / 2.0,
    image_width: crop.width,
    radius_percent: 0.0,
  }
}
fn layer(id: u32, frame_id: u32, z_index: i32) -> WorkspaceLayer {
  WorkspaceLayer {
    id: LayerId(id),
    frame_id: FrameId(frame_id),
    rect: n(0.1, 0.1, 0.5, 0.5),
    radius_percent: 0.0,
    z_index,
  }
}
#[test]
fn screenshot_is_one_frame_with_multiple_layers() {
  let s = WorkspaceScene::screenshot(
    r(0.0, 0.0, 100.0, 100.0),
    r(0.0, 0.0, 100.0, 100.0),
    vec![layer(1, 0, 0), layer(2, 0, 1)],
  )
  .unwrap();
  assert_eq!(s.frames.len(), 1);
  assert_eq!(s.layers.len(), 2);
}
#[test]
fn baked_video_is_one_frame() {
  let s = WorkspaceScene::baked_video(
    r(0.0, 0.0, 100.0, 100.0),
    r(0.0, 0.0, 100.0, 100.0),
    vec![layer(1, 0, 0)],
  )
  .unwrap();
  assert_eq!(s.kind, WorkspaceKind::BakedVideo);
}
#[test]
fn split_video_has_two_independent_frames() {
  let frames = vec![
    WorkspaceFrame {
      id: FrameId(0),
      rect: r(0.0, 0.0, 50.0, 100.0),
      radius_percent: 0.0,
    },
    WorkspaceFrame {
      id: FrameId(1),
      rect: r(50.0, 0.0, 50.0, 100.0),
      radius_percent: 0.0,
    },
  ];
  let s =
    WorkspaceScene::split_video(r(0.0, 0.0, 100.0, 100.0), frames, vec![layer(1, 1, 0)]).unwrap();
  assert_eq!(s.frames.len(), 2);
  assert_eq!(
    s.frames[1]
      .rect
      .normalized(s.layers[0].rect.x, 0.0, 0.0, 0.0)
      .x,
    55.0
  );
}
#[test]
fn layer_resize_uses_one_transform_for_crop_and_image() {
  let start = geometry(n(0.2, 0.3, 0.4, 0.5));
  let result = apply_layer_gesture(start, GestureOperation::Resize, (0.1, -0.1), 0.5);
  assert!((result.crop.x - 0.3).abs() < 1e-9);
  assert!((result.crop.y - 0.2).abs() < 1e-9);
  assert_eq!((result.crop.width, result.crop.height), (0.2, 0.25));
  assert_eq!(result.image_width, 0.2);
  assert!((result.image_center_x - 0.4).abs() < 1e-9);
  assert!((result.image_center_y - 0.325).abs() < 1e-9);
}
#[test]
fn layer_gesture_allows_off_canvas_move() {
  let result = apply_layer_gesture(
    geometry(n(0.1, 0.1, 0.2, 0.2)),
    GestureOperation::Move,
    (-1.0, 2.0),
    1.0,
  );
  assert_eq!((result.crop.x, result.crop.y), (-0.9, 2.1));
  assert_eq!((result.image_center_x, result.image_center_y), (-0.8, 2.2));
}
#[test]
fn frame_resize_top_edge_is_undo_symmetric() {
  let s = WorkspaceScene::screenshot(
    r(0.0, 0.0, 400.0, 400.0),
    r(20.0, 30.0, 200.0, 160.0),
    vec![],
  )
  .unwrap();
  let grown = s
    .resized_frame(FrameId(0), FRAME_EDGE_TOP, (-0.0, -20.0))
    .unwrap();
  let restored = grown
    .scene
    .resized_frame(FrameId(0), FRAME_EDGE_TOP, (0.0, 20.0))
    .unwrap();
  assert_eq!(restored.new_rect, s.frame(FrameId(0)).unwrap().rect);
  assert_eq!(restored.output_size, (200, 160));
}
#[test]
fn split_resize_only_changes_selected_frame() {
  let frames = vec![
    WorkspaceFrame {
      id: FrameId(0),
      rect: r(0.0, 0.0, 200.0, 200.0),
      radius_percent: 0.0,
    },
    WorkspaceFrame {
      id: FrameId(1),
      rect: r(220.0, 0.0, 200.0, 200.0),
      radius_percent: 0.0,
    },
  ];
  let s = WorkspaceScene::split_video(r(0.0, 0.0, 500.0, 300.0), frames, vec![]).unwrap();
  let result = s
    .resized_frame(FrameId(1), FRAME_EDGE_RIGHT, (40.0, 0.0))
    .unwrap();
  assert_eq!(
    result.scene.frame(FrameId(0)).unwrap().rect,
    s.frame(FrameId(0)).unwrap().rect
  );
  assert_eq!(result.new_rect.width, 240.0);
}
#[test]
fn baked_resize_rebases_layer_without_moving_pixels() {
  let layer = WorkspaceLayer {
    id: LayerId(1),
    frame_id: FrameId(0),
    rect: n(0.25, 0.25, 0.5, 0.5),
    radius_percent: 0.0,
    z_index: 0,
  };
  let s = WorkspaceScene::baked_video(
    r(0.0, 0.0, 400.0, 400.0),
    r(100.0, 100.0, 200.0, 200.0),
    vec![layer],
  )
  .unwrap();
  let before = s.frames[0].rect.normalized(
    s.layers[0].rect.x,
    s.layers[0].rect.y,
    s.layers[0].rect.width,
    s.layers[0].rect.height,
  );
  let result = s
    .resized_frame(FrameId(0), FRAME_EDGE_RIGHT, (100.0, 0.0))
    .unwrap();
  let after = result.scene.frames[0].rect.normalized(
    result.scene.layers[0].rect.x,
    result.scene.layers[0].rect.y,
    result.scene.layers[0].rect.width,
    result.scene.layers[0].rect.height,
  );
  assert_eq!(after, before);
  assert_eq!(result.scene.layers[0].rect.x, 1.0 / 6.0);
}
#[test]
fn centered_resize_keeps_frame_center() {
  let s = WorkspaceScene::screenshot(
    r(0.0, 0.0, 400.0, 400.0),
    r(100.0, 100.0, 200.0, 200.0),
    vec![],
  )
  .unwrap();
  let result = s
    .resized_frame(
      FrameId(0),
      FRAME_EDGE_RIGHT | FRAME_EDGE_CENTERED,
      (20.0, 0.0),
    )
    .unwrap();
  assert_eq!(result.new_rect, r(80.0, 100.0, 240.0, 200.0));
}
#[test]
fn frame_resize_respects_max_area() {
  let s = WorkspaceScene::screenshot(
    r(0.0, 0.0, 20_000.0, 20_000.0),
    r(0.0, 0.0, 12_000.0, 12_000.0),
    vec![],
  )
  .unwrap();
  let result = s
    .resized_frame(
      FrameId(0),
      FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
      (10_000.0, 10_000.0),
    )
    .unwrap();
  assert!(result.new_rect.width * result.new_rect.height <= FRAME_MAX_AREA);
  assert!(result.output_size.0 >= FRAME_MIN_SIZE as u32);
}
#[test]
fn layer_geometry_rebase_preserves_absolute_image_transform() {
  let old = r(100.0, 50.0, 200.0, 100.0);
  let new = r(50.0, 25.0, 400.0, 200.0);
  let start = LayerGeometry {
    crop: n(0.25, 0.25, 0.5, 0.5),
    image_center_x: 0.5,
    image_center_y: 0.5,
    image_width: 0.75,
    radius_percent: 12.0,
  };
  let rebased = rebase_layer_geometry(start, old, new);
  assert_eq!(rebased.crop, n(0.25, 0.25, 0.25, 0.25));
  assert_eq!(
    (rebased.image_center_x, rebased.image_center_y),
    (0.375, 0.375)
  );
  assert_eq!(rebased.image_width, 0.375);
  assert_eq!(rebased.radius_percent, 12.0);
}

#[test]
fn display_fit_rebase_preserves_displayed_bounds() {
  let displayed = DisplayRect {
    x: 140.0,
    y: 100.0,
    width: 720.0,
    height: 360.0,
  };
  let rebased = rebase_display_fit((1_000.0, 700.0), displayed, 8.0);
  assert_eq!(
    rebased.fit,
    DisplayRect {
      x: 8.0,
      y: 104.0,
      width: 984.0,
      height: 492.0,
    }
  );
  assert!((rebased.zoom - 720.0 / 984.0).abs() < 0.000_001);
  assert_eq!(rebased.pan_x, 0.0);
  assert_eq!(rebased.pan_y, -70.0);
}

#[test]
fn canvas_fit_preserves_absolute_layer_geometry() {
  let layer = LayerGeometry {
    crop: n(0.25, -0.5, 0.5, 1.0),
    image_center_x: 0.5,
    image_center_y: 0.0,
    image_width: 0.75,
    radius_percent: 12.0,
  };
  let ((width, height), layers) = fit_canvas_to_layers((400, 200), &[layer]);
  assert_eq!((width, height), (400, 300));
  let fitted = layers[0];
  assert_eq!(fitted.crop, n(0.25, 0.0, 0.5, 2.0 / 3.0));
  assert_eq!(fitted.image_center_x, 0.5);
  assert_eq!(fitted.image_center_y, 1.0 / 3.0);
  assert_eq!(fitted.image_width, 0.75);
  assert_eq!(fitted.radius_percent, 12.0);
}

#[test]
fn crop_move_is_clamped_without_changing_size() {
  let crop = n(0.2, 0.25, 0.4, 0.5);
  let image = n(0.1, 0.1, 0.8, 0.8);
  let moved = apply_crop_move(crop, image, (1.0, -1.0));
  assert_eq!(moved, n(0.5, 0.1, 0.4, 0.5));
}

#[test]
fn crop_resize_is_clamped_to_image_and_preserves_image() {
  let crop = n(0.2, 0.2, 0.4, 0.4);
  let image = n(0.1, 0.1, 0.8, 0.8);
  let resized = apply_crop_resize(
    crop,
    image,
    FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
    (1.0, 1.0),
    false,
  );
  assert_eq!(resized, n(0.2, 0.2, 0.7, 0.7));
  let centered = apply_crop_resize(
    crop,
    image,
    FRAME_EDGE_RIGHT | FRAME_EDGE_BOTTOM,
    (0.1, 0.1),
    true,
  );
  assert!((centered.x - 0.1).abs() < 1e-9);
  assert!((centered.y - 0.1).abs() < 1e-9);
  assert!((centered.width - 0.6).abs() < 1e-9);
  assert!((centered.height - 0.6).abs() < 1e-9);
}
