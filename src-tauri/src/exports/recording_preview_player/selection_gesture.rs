// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{PlaybackMode, PreviewPlayerManager, RecordingSelectionGesture};
use super::{AUTO_FIT_COMMIT_EDGE, AUTO_FIT_MOVE_EDGE};
use crate::exports::preview_platform::{
  workspace_editor::{
    apply_layer_gesture, fit_canvas_to_layers, GestureOperation as WorkspaceGestureOperation,
    LayerGeometry, NormalizedRect,
  },
  SelectionGestureOperation, SelectionGesturePhase,
};

impl PreviewPlayerManager {
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  #[allow(clippy::too_many_arguments)] // Native editor callback fields stay separate.
  pub(super) fn handle_selection_gesture(
    &mut self,
    phase: SelectionGesturePhase,
    layer_id: u32,
    operation: SelectionGestureOperation,
    edges: u32,
    scale: f64,
    delta_x: f64,
    delta_y: f64,
  ) -> Result<(), String> {
    let settings = self
      .sources
      .as_ref()
      .and_then(|sources| sources.composition_settings.clone())
      .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?;
    match phase {
      SelectionGesturePhase::Begin => {
        let snapshot = settings
          .read()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())?
          .clone();
        // Crop display composition is controlled by React's uncropped
        // preview output. Do not freeze the old selected layer here: the
        // selection OSC changes synchronously on mouse-down, and React must
        // be able to present the newly selected layer's uncropped pixels
        // before the first crop update arrives.
        if matches!(
          operation,
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize
        ) {
          self.selection_gesture = None;
          return Ok(());
        }
        self.selection_gesture = Some(RecordingSelectionGesture { snapshot });
        Ok(())
      }
      SelectionGesturePhase::Update | SelectionGesturePhase::End => {
        if matches!(
          operation,
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize
        ) {
          // Crop pixels are mirrored by React's uncropped composition. Keep
          // this native manager out of the gesture snapshot so each selected
          // layer can present immediately during the crop interaction.
          return Ok(());
        }
        let ending = matches!(phase, SelectionGesturePhase::End);
        if self.selection_gesture.is_none() {
          let snapshot = settings
            .read()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())?
            .clone();
          self.selection_gesture = Some(RecordingSelectionGesture { snapshot });
        }
        if operation == SelectionGestureOperation::Move && edges & AUTO_FIT_COMMIT_EDGE != 0 {
          let current = settings
            .read()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())?
            .clone();
          if let Some(gesture) = self.selection_gesture.as_mut() {
            gesture.snapshot = current;
          }
          return Ok(());
        }
        let Some(gesture) = self.selection_gesture.as_ref() else {
          return Ok(());
        };
        let snapshot = &gesture.snapshot;
        let mut next = snapshot.clone();
        if matches!(
          operation,
          SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius
        ) {
          if operation == SelectionGestureOperation::FrameRadius {
            return Ok(());
          }
          let Some(mut scene) = self.workspace_scene.clone() else {
            return Ok(());
          };
          let output = match layer_id {
            0 => &snapshot.recording_output.primary,
            1 if !snapshot.bake_camera => &snapshot.recording_output.camera,
            _ => return Ok(()),
          };
          let Some(frame) = scene.frames.iter_mut().find(|frame| frame.id.0 == layer_id) else {
            return Ok(());
          };
          frame.rect.width = f64::from(output.width);
          frame.rect.height = f64::from(output.height);
          let (scene, recording_output, camera_overlay) =
            super::super::preview_workspace_model::resize_recording_frame(
              &scene,
              &snapshot.recording_output,
              snapshot.camera_overlay,
              snapshot.bake_camera,
              layer_id,
              edges,
              (delta_x, delta_y),
            )?;
          next.recording_output = recording_output;
          next.camera_overlay = camera_overlay;
          self.workspace_scene = Some(scene);
          *settings
            .write()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
          // The retained native workspace updates media and OSC in the same
          // command buffer. React mirrors this state, and the final layout
          // performs the one post-gesture decoder reconciliation.
          if ending {
            self.selection_gesture = None;
          }
          return Ok(());
        }
        if snapshot.bake_camera && layer_id == 1 {
          let start = snapshot.camera_overlay;
          let operation = match operation {
            SelectionGestureOperation::Move => WorkspaceGestureOperation::Move,
            SelectionGestureOperation::Resize => WorkspaceGestureOperation::Resize,
            SelectionGestureOperation::Radius => WorkspaceGestureOperation::Radius,
            SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius => {
              return Ok(())
            }
            SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize => {
              unreachable!("crop gestures are mirrored by the frontend")
            }
          };
          let mut geometry = apply_layer_gesture(
            LayerGeometry {
              crop: NormalizedRect {
                x: start.frame_x_percent / 100.0,
                y: start.frame_y_percent / 100.0,
                width: start.frame_width_percent / 100.0,
                height: start.frame_height_percent / 100.0,
              },
              image_center_x: start.camera_x_percent / 100.0,
              image_center_y: start.camera_y_percent / 100.0,
              image_width: start.camera_width_percent / 100.0,
              radius_percent: start.radius_percent,
            },
            operation,
            (delta_x, delta_y),
            scale,
          );
          if operation == WorkspaceGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
            let primary = &snapshot.recording_output.primary;
            let primary_geometry = LayerGeometry {
              crop: NormalizedRect {
                x: primary.screenshot_crop_x_percent / 100.0,
                y: primary.screenshot_crop_y_percent / 100.0,
                width: primary.screenshot_crop_width_percent / 100.0,
                height: primary.screenshot_crop_height_percent / 100.0,
              },
              image_center_x: primary.screenshot_image_x_percent / 100.0,
              image_center_y: primary.screenshot_image_y_percent / 100.0,
              image_width: primary.screenshot_image_width_percent / 100.0,
              radius_percent: primary.radius_percent,
            };
            let ((width, height), fitted) = fit_canvas_to_layers(
              (primary.width, primary.height),
              &[primary_geometry, geometry],
            );
            let fitted_primary = fitted[0];
            geometry = fitted[1];
            next.recording_output.primary.width = width;
            next.recording_output.primary.height = height;
            next.recording_output.primary.screenshot_crop_x_percent = fitted_primary.crop.x * 100.0;
            next.recording_output.primary.screenshot_crop_y_percent = fitted_primary.crop.y * 100.0;
            next.recording_output.primary.screenshot_crop_width_percent =
              fitted_primary.crop.width * 100.0;
            next.recording_output.primary.screenshot_crop_height_percent =
              fitted_primary.crop.height * 100.0;
            next.recording_output.primary.screenshot_image_x_percent =
              fitted_primary.image_center_x * 100.0;
            next.recording_output.primary.screenshot_image_y_percent =
              fitted_primary.image_center_y * 100.0;
            next.recording_output.primary.screenshot_image_width_percent =
              fitted_primary.image_width * 100.0;
          }
          next.camera_overlay.frame_x_percent = geometry.crop.x * 100.0;
          next.camera_overlay.frame_y_percent = geometry.crop.y * 100.0;
          next.camera_overlay.frame_width_percent = geometry.crop.width * 100.0;
          next.camera_overlay.frame_height_percent = geometry.crop.height * 100.0;
          next.camera_overlay.camera_x_percent = geometry.image_center_x * 100.0;
          next.camera_overlay.camera_y_percent = geometry.image_center_y * 100.0;
          next.camera_overlay.camera_width_percent = geometry.image_width * 100.0;
          next.camera_overlay.radius_percent = geometry.radius_percent;
          *settings
            .write()
            .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
          if edges & AUTO_FIT_MOVE_EDGE != 0 {
            if ending {
              self.selection_gesture = None;
            }
            return Ok(());
          }
          let result = self.refresh_selection_preview(layer_id);
          if ending {
            self.selection_gesture = None;
          }
          return result;
        }
        let (start, output) = match layer_id {
          0 => (
            &snapshot.recording_output.primary,
            &mut next.recording_output.primary,
          ),
          1 => (
            &snapshot.recording_output.camera,
            &mut next.recording_output.camera,
          ),
          _ => return Ok(()),
        };
        let operation = match operation {
          SelectionGestureOperation::Move => WorkspaceGestureOperation::Move,
          SelectionGestureOperation::Resize => WorkspaceGestureOperation::Resize,
          SelectionGestureOperation::Radius => WorkspaceGestureOperation::Radius,
          SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius => {
            return Ok(())
          }
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize => {
            unreachable!("crop gestures are mirrored by the frontend")
          }
        };
        let mut geometry = apply_layer_gesture(
          LayerGeometry {
            crop: NormalizedRect {
              x: start.screenshot_crop_x_percent / 100.0,
              y: start.screenshot_crop_y_percent / 100.0,
              width: start.screenshot_crop_width_percent / 100.0,
              height: start.screenshot_crop_height_percent / 100.0,
            },
            image_center_x: start.screenshot_image_x_percent / 100.0,
            image_center_y: start.screenshot_image_y_percent / 100.0,
            image_width: start.screenshot_image_width_percent / 100.0,
            radius_percent: start.radius_percent,
          },
          operation,
          (delta_x, delta_y),
          scale,
        );
        if operation == WorkspaceGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
          let ((width, height), fitted) =
            fit_canvas_to_layers((start.width, start.height), &[geometry]);
          geometry = fitted[0];
          output.width = width;
          output.height = height;
        }
        output.screenshot_crop_x_percent = geometry.crop.x * 100.0;
        output.screenshot_crop_y_percent = geometry.crop.y * 100.0;
        output.screenshot_crop_width_percent = geometry.crop.width * 100.0;
        output.screenshot_crop_height_percent = geometry.crop.height * 100.0;
        output.screenshot_image_x_percent = geometry.image_center_x * 100.0;
        output.screenshot_image_y_percent = geometry.image_center_y * 100.0;
        output.screenshot_image_width_percent = geometry.image_width * 100.0;
        output.radius_percent = geometry.radius_percent;
        *settings
          .write()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())? = next;
        if edges & AUTO_FIT_MOVE_EDGE != 0 {
          if ending {
            self.selection_gesture = None;
          }
          return Ok(());
        }
        let result = self.refresh_selection_preview(layer_id);
        if ending {
          self.selection_gesture = None;
        }
        result
      }
      SelectionGesturePhase::Cancel => {
        let Some(gesture) = self.selection_gesture.take() else {
          return Ok(());
        };
        *settings
          .write()
          .map_err(|_| "The recording preview composition is unavailable".to_owned())? =
          gesture.snapshot;
        self.restart(PlaybackMode::InteractiveStill)
      }
    }
  }
}
