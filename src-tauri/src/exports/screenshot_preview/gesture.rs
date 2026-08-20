// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::preview_platform::workspace_editor::{
  apply_layer_gesture, GestureOperation as WorkspaceGestureOperation, LayerGeometry,
  NormalizedRect, WorldRect,
};
use super::super::preview_platform::{SelectionGestureOperation, SelectionGesturePhase};
use super::super::ScreenshotWorkspaceOutputSettings;
use super::geometry::fit_workspace_to_items;
use super::state::PreviewManager;

const AUTO_FIT_MOVE_EDGE: u32 = 1 << 17;
const AUTO_FIT_COMMIT_EDGE: u32 = 1 << 18;

// Only the macOS layout reads the gesture's presentation ownership.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(super) struct SelectionGestureOverride {
  pub(super) native_workspace_owns_presentation: bool,
  pub(super) operation: SelectionGestureOperation,
  pub(super) snapshot: ScreenshotWorkspaceOutputSettings,
}

impl PreviewManager {
  #[allow(clippy::too_many_arguments)]
  pub(super) fn handle_selection_gesture(
    &mut self,
    phase: SelectionGesturePhase,
    pane_index: u32,
    operation: SelectionGestureOperation,
    edges: u32,
    scale: f64,
    delta_x: f64,
    delta_y: f64,
  ) -> Result<(), String> {
    let current = if matches!(&phase, SelectionGesturePhase::Begin) {
      self.react_output.clone().or_else(|| self.output.clone())
    } else {
      self.output.clone()
    };
    let Some(current) = current else {
      return Ok(());
    };
    match phase {
      SelectionGesturePhase::Begin => {
        // The OSC is derived from React's latest layout. Rebase native pixel
        // composition to that exact same snapshot before accepting pointer
        // deltas so both Metal layers share one gesture origin.
        self.output = Some(current.clone());
        // Crop mode is mirrored by React: selecting another layer must be
        // allowed to replace the display-only uncropped composition before
        // the first crop pointer update. Retaining this snapshot would keep
        // the previously selected layer's uncropped pixels alive while the
        // native OSC had already moved to the new layer.
        if matches!(
          operation,
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize
        ) {
          self.selection_gesture = None;
          return Ok(());
        }
        self.selection_gesture = Some(SelectionGestureOverride {
          native_workspace_owns_presentation: false,
          operation,
          snapshot: current.clone(),
        });
        return if operation == SelectionGestureOperation::FrameResize {
          Ok(())
        } else {
          self.present_batch()
        };
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
        // A structural layout may acknowledge the unchanged Begin snapshot
        // before the first mouse movement arrives. Re-establish the native
        // gesture from the still-current output so that live pixels do not
        // depend on winning that harmless race.
        self
          .selection_gesture
          .get_or_insert_with(|| SelectionGestureOverride {
            native_workspace_owns_presentation: false,
            operation,
            snapshot: current.clone(),
          });
        if operation == SelectionGestureOperation::Move && edges & AUTO_FIT_COMMIT_EDGE != 0 {
          if let Some(gesture) = self.selection_gesture.as_mut() {
            // Option release accepts the native workspace geometry as the
            // origin for the remainder of this same pointer/history gesture.
            gesture.native_workspace_owns_presentation = false;
            gesture.snapshot = current;
          }
          return Ok(());
        }
        if let Some(gesture) = self.selection_gesture.as_mut() {
          gesture.native_workspace_owns_presentation =
            operation == SelectionGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0;
        }
        let Some(gesture) = self.selection_gesture.as_ref() else {
          return Ok(());
        };
        let mut next = gesture.snapshot.clone();
        let snapshot = gesture.snapshot.clone();
        if operation == SelectionGestureOperation::FrameResize {
          let viewport = self.workspace_scene.as_ref().map_or(
            WorldRect {
              x: 0.0,
              y: 0.0,
              width: f64::from(snapshot.canvas.width),
              height: f64::from(snapshot.canvas.height),
            },
            |scene| scene.viewport,
          );
          let scene = super::preview_workspace_model::screenshot_scene(
            viewport,
            &snapshot,
            self
              .workspace_scene
              .as_ref()
              .map_or(0, |scene| scene.revision),
          )?;
          let (scene, resized) = super::preview_workspace_model::resize_screenshot_frame(
            &scene,
            &snapshot,
            edges,
            (delta_x, delta_y),
          )?;
          next = resized;
          self.workspace_scene = Some(scene);
          self.output = Some(next);
          if matches!(phase, SelectionGesturePhase::End) {
            self.selection_gesture = None;
          } else {
            self.selection_gesture = Some(SelectionGestureOverride {
              native_workspace_owns_presentation: false,
              operation,
              snapshot,
            });
          }
          // macOS recomposes the resized canvas in its retained GPU workspace;
          // Windows has no such presenter, so the re-composition happens here,
          // in the same input, and its present publishes the pane box the
          // native drag deferred.
          #[cfg(target_os = "windows")]
          return self.present_batch();
          #[cfg(not(target_os = "windows"))]
          return Ok(());
        }
        if operation == SelectionGestureOperation::FrameRadius {
          next.canvas.background_radius_percent = scale.clamp(0.0, 50.0);
          self.output = Some(next);
          if matches!(phase, SelectionGesturePhase::End) {
            self.selection_gesture = None;
          } else {
            self.selection_gesture = Some(SelectionGestureOverride {
              native_workspace_owns_presentation: false,
              operation,
              snapshot,
            });
          }
          return self.present_batch();
        }
        let Some(start) = snapshot
          .items
          .get(pane_index as usize)
          .map(|item| &item.output)
        else {
          return Ok(());
        };
        let background_radius_percent = next.canvas.background_radius_percent;
        let Some(item) = next.items.get_mut(pane_index as usize) else {
          return Ok(());
        };
        let workspace_operation = match operation {
          SelectionGestureOperation::Move => WorkspaceGestureOperation::Move,
          SelectionGestureOperation::Resize => WorkspaceGestureOperation::Resize,
          SelectionGestureOperation::Radius => WorkspaceGestureOperation::Radius,
          SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius => {
            unreachable!("frame gestures are handled before selecting an item")
          }
          SelectionGestureOperation::CropMove | SelectionGestureOperation::CropResize => {
            unreachable!("crop gestures are mirrored by the frontend")
          }
        };
        let geometry = apply_layer_gesture(
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
          workspace_operation,
          (delta_x, delta_y),
          scale,
        );
        item.output.screenshot_crop_x_percent = geometry.crop.x * 100.0;
        item.output.screenshot_crop_y_percent = geometry.crop.y * 100.0;
        item.output.screenshot_crop_width_percent = geometry.crop.width * 100.0;
        item.output.screenshot_crop_height_percent = geometry.crop.height * 100.0;
        item.output.screenshot_image_x_percent = geometry.image_center_x * 100.0;
        item.output.screenshot_image_y_percent = geometry.image_center_y * 100.0;
        item.output.screenshot_image_width_percent = geometry.image_width * 100.0;
        item.output.radius_percent = geometry.radius_percent;
        // Keep the legacy flattened fields consistent with the selected item;
        // composition still reads global canvas properties from this value.
        let moved_output = item.output.clone();
        next.canvas = moved_output.clone();
        next.canvas.background_radius_percent = background_radius_percent;
        if operation == SelectionGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
          next = fit_workspace_to_items(&snapshot, pane_index as usize, &moved_output);
        }
        self.output = Some(next);
        if matches!(phase, SelectionGesturePhase::End) {
          // Mouse-up carries the authoritative final transform. Applying it
          // from the gesture snapshot keeps a dropped final move (for example
          // when Command snapping is released) from shifting on commit.
          self.selection_gesture = None;
        } else {
          self.selection_gesture = Some(SelectionGestureOverride {
            native_workspace_owns_presentation: operation == SelectionGestureOperation::Move
              && edges & AUTO_FIT_MOVE_EDGE != 0,
            operation,
            snapshot,
          });
        }
        if operation == SelectionGestureOperation::Move && edges & AUTO_FIT_MOVE_EDGE != 0 {
          // The native workspace presenter owns this complete live scene:
          // frame resize, selected-layer movement and OSC are encoded from
          // one immutable gesture snapshot. Replacing it here would race a
          // differently normalized but semantically equivalent React scene.
          // Windows has no retained presenter: the fitted canvas is composed
          // here, and `native_workspace_owns_presentation` still keeps React's
          // equivalent layouts from re-presenting it meanwhile.
          #[cfg(target_os = "windows")]
          return self.present_batch();
          #[cfg(not(target_os = "windows"))]
          return Ok(());
        }
        return self.present_batch();
      }
      SelectionGesturePhase::Cancel => {
        if let Some(gesture) = self.selection_gesture.take() {
          self.output = Some(gesture.snapshot);
          return self.present_batch();
        }
      }
    }
    Ok(())
  }
}
