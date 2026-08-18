// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Converts screenshot and recording settings into the one native workspace
//! model consumed by both GPU backends.

use super::preview_platform::workspace_editor::{
  rebase_layer_geometry, FrameId, LayerGeometry, LayerId, NormalizedRect, WorkspaceFrame,
  WorkspaceLayer, WorkspaceScene, WorldRect,
};
use super::{CameraOverlaySettings, RecordingOutputSettings, ScreenshotWorkspaceOutputSettings};
use crate::screenshots::ScreenshotOutputSettings;

#[derive(Clone, Copy)]
pub(crate) struct WorkspacePane {
  pub id: u32,
  pub rect: WorldRect,
}

fn layer(
  id: u32,
  frame_id: u32,
  output: &ScreenshotOutputSettings,
  z_index: i32,
) -> WorkspaceLayer {
  WorkspaceLayer {
    id: LayerId(id),
    frame_id: FrameId(frame_id),
    rect: NormalizedRect {
      x: output.screenshot_crop_x_percent / 100.0,
      y: output.screenshot_crop_y_percent / 100.0,
      width: output.screenshot_crop_width_percent / 100.0,
      height: output.screenshot_crop_height_percent / 100.0,
    },
    radius_percent: output.radius_percent,
    z_index,
  }
}

fn output_geometry(output: &ScreenshotOutputSettings) -> LayerGeometry {
  LayerGeometry {
    crop: NormalizedRect {
      x: output.screenshot_crop_x_percent / 100.0,
      y: output.screenshot_crop_y_percent / 100.0,
      width: output.screenshot_crop_width_percent / 100.0,
      height: output.screenshot_crop_height_percent / 100.0,
    },
    image_center_x: output.screenshot_image_x_percent / 100.0,
    image_center_y: output.screenshot_image_y_percent / 100.0,
    image_width: output.screenshot_image_width_percent / 100.0,
    radius_percent: output.radius_percent,
  }
}

fn apply_output_geometry(output: &mut ScreenshotOutputSettings, geometry: LayerGeometry) {
  output.screenshot_crop_x_percent = geometry.crop.x * 100.0;
  output.screenshot_crop_y_percent = geometry.crop.y * 100.0;
  output.screenshot_crop_width_percent = geometry.crop.width * 100.0;
  output.screenshot_crop_height_percent = geometry.crop.height * 100.0;
  output.screenshot_image_x_percent = geometry.image_center_x * 100.0;
  output.screenshot_image_y_percent = geometry.image_center_y * 100.0;
  output.screenshot_image_width_percent = geometry.image_width * 100.0;
  output.radius_percent = geometry.radius_percent;
}

fn camera_geometry(camera: CameraOverlaySettings) -> LayerGeometry {
  LayerGeometry {
    crop: NormalizedRect {
      x: camera.frame_x_percent / 100.0,
      y: camera.frame_y_percent / 100.0,
      width: camera.frame_width_percent / 100.0,
      height: camera.frame_height_percent / 100.0,
    },
    image_center_x: camera.camera_x_percent / 100.0,
    image_center_y: camera.camera_y_percent / 100.0,
    image_width: camera.camera_width_percent / 100.0,
    radius_percent: camera.radius_percent,
  }
}

fn apply_camera_geometry(camera: &mut CameraOverlaySettings, geometry: LayerGeometry) {
  camera.frame_x_percent = geometry.crop.x * 100.0;
  camera.frame_y_percent = geometry.crop.y * 100.0;
  camera.frame_width_percent = geometry.crop.width * 100.0;
  camera.frame_height_percent = geometry.crop.height * 100.0;
  camera.camera_x_percent = geometry.image_center_x * 100.0;
  camera.camera_y_percent = geometry.image_center_y * 100.0;
  camera.camera_width_percent = geometry.image_width * 100.0;
  camera.radius_percent = geometry.radius_percent;
}

pub(crate) fn screenshot_scene(
  viewport: WorldRect,
  output: &ScreenshotWorkspaceOutputSettings,
  revision: u64,
) -> Result<WorkspaceScene, String> {
  let layers = output
    .items
    .iter()
    .enumerate()
    .map(|(index, item)| layer(index as u32, 0, &item.output, index as i32))
    .collect();
  let mut scene = WorkspaceScene::screenshot(
    viewport,
    WorldRect {
      x: 0.0,
      y: 0.0,
      width: f64::from(output.canvas.width),
      height: f64::from(output.canvas.height),
    },
    layers,
  )?;
  scene.frames[0].radius_percent = output.canvas.background_radius_percent;
  scene.revision = revision;
  Ok(scene)
}

pub(crate) fn recording_scene(
  viewport: WorldRect,
  panes: &[WorkspacePane],
  bake_camera: bool,
  camera: CameraOverlaySettings,
  output: &RecordingOutputSettings,
  revision: u64,
) -> Result<WorkspaceScene, String> {
  let mut scene = if bake_camera {
    let frame_id = panes
      .iter()
      .find(|pane| pane.id == 0)
      .or_else(|| panes.first())
      .ok_or_else(|| "recording workspace must contain a frame".to_owned())?
      .id;
    let layers = vec![
      layer(0, frame_id, &output.primary, 0),
      WorkspaceLayer {
        id: LayerId(1),
        frame_id: FrameId(frame_id),
        rect: NormalizedRect {
          x: camera.frame_x_percent / 100.0,
          y: camera.frame_y_percent / 100.0,
          width: camera.frame_width_percent / 100.0,
          height: camera.frame_height_percent / 100.0,
        },
        radius_percent: camera.radius_percent,
        z_index: i32::from(output.camera_on_top),
      },
    ];
    WorkspaceScene::baked_video(
      viewport,
      WorldRect {
        x: 0.0,
        y: 0.0,
        width: f64::from(output.primary.width),
        height: f64::from(output.primary.height),
      },
      layers,
    )?
  } else {
    let left = panes
      .iter()
      .map(|pane| pane.rect.x)
      .fold(f64::INFINITY, f64::min);
    let top = panes
      .iter()
      .map(|pane| pane.rect.y)
      .fold(f64::INFINITY, f64::min);
    let fit = panes
      .iter()
      .find_map(|pane| {
        let width = if pane.id == 0 {
          output.primary.width
        } else {
          output.camera.width
        };
        (width > 0 && pane.rect.width > 0.0).then_some(pane.rect.width / f64::from(width))
      })
      .unwrap_or(1.0)
      .max(f64::EPSILON);
    let frames = panes
      .iter()
      .map(|pane| {
        let output = if pane.id == 0 {
          &output.primary
        } else {
          &output.camera
        };
        WorkspaceFrame {
          id: FrameId(pane.id),
          rect: WorldRect {
            x: (pane.rect.x - left) / fit,
            y: (pane.rect.y - top) / fit,
            width: f64::from(output.width),
            height: f64::from(output.height),
          },
          radius_percent: 0.0,
        }
      })
      .collect();
    let layers = panes
      .iter()
      .map(|pane| {
        let output = if pane.id == 0 {
          &output.primary
        } else {
          &output.camera
        };
        layer(pane.id, pane.id, output, 0)
      })
      .collect();
    WorkspaceScene::split_video(viewport, frames, layers)?
  };
  scene.revision = revision;
  Ok(scene)
}

pub(crate) fn resize_screenshot_frame(
  scene: &WorkspaceScene,
  output: &ScreenshotWorkspaceOutputSettings,
  edges: u32,
  normalized_delta: (f64, f64),
) -> Result<(WorkspaceScene, ScreenshotWorkspaceOutputSettings), String> {
  let frame = scene.frame(FrameId(0)).ok_or("screenshot frame missing")?;
  let resized = scene.resized_frame(
    FrameId(0),
    edges,
    (
      normalized_delta.0 * frame.rect.width,
      normalized_delta.1 * frame.rect.height,
    ),
  )?;
  let mut next = output.clone();
  next.canvas.width = resized.output_size.0;
  next.canvas.height = resized.output_size.1;
  for item in &mut next.items {
    let geometry = rebase_layer_geometry(
      output_geometry(&item.output),
      resized.old_rect,
      resized.new_rect,
    );
    apply_output_geometry(&mut item.output, geometry);
    item.output.width = resized.output_size.0;
    item.output.height = resized.output_size.1;
  }
  let canvas_geometry = rebase_layer_geometry(
    output_geometry(&output.canvas),
    resized.old_rect,
    resized.new_rect,
  );
  apply_output_geometry(&mut next.canvas, canvas_geometry);
  Ok((resized.scene, next))
}

pub(crate) fn resize_recording_frame(
  scene: &WorkspaceScene,
  output: &RecordingOutputSettings,
  camera: CameraOverlaySettings,
  bake_camera: bool,
  frame_id: u32,
  edges: u32,
  normalized_delta: (f64, f64),
) -> Result<
  (
    WorkspaceScene,
    RecordingOutputSettings,
    CameraOverlaySettings,
  ),
  String,
> {
  let id = FrameId(frame_id);
  let frame = scene.frame(id).ok_or("recording frame missing")?;
  let resized = scene.resized_frame(
    id,
    edges,
    (
      normalized_delta.0 * frame.rect.width,
      normalized_delta.1 * frame.rect.height,
    ),
  )?;
  let mut next_output = output.clone();
  let selected = if frame_id == 0 {
    &mut next_output.primary
  } else {
    &mut next_output.camera
  };
  let geometry = rebase_layer_geometry(
    output_geometry(selected),
    resized.old_rect,
    resized.new_rect,
  );
  apply_output_geometry(selected, geometry);
  selected.width = resized.output_size.0;
  selected.height = resized.output_size.1;

  let mut next_camera = camera;
  if bake_camera && frame_id == 0 {
    let geometry =
      rebase_layer_geometry(camera_geometry(camera), resized.old_rect, resized.new_rect);
    apply_camera_geometry(&mut next_camera, geometry);
  }
  Ok((resized.scene, next_output, next_camera))
}
