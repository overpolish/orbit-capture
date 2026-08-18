// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native screenshot editing preview.
//!
//! The screenshot is a single static image, so the whole editing loop is one
//! GPU composition into the same native pane surface the recording preview
//! uses: the source uploads once (the presenter caches it by token), and each
//! settings change is a uniform-only compute pass. No pixels ever cross IPC.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};

use super::preview_platform::workspace_editor::{
  apply_layer_gesture, GestureOperation as WorkspaceGestureOperation, LayerGeometry,
  NormalizedRect, WorkspaceScene, WorldRect,
};
use super::preview_platform::{
  PreviewSelection, PreviewSurfaceRect, RecordingPreviewSurface, SelectionGestureOperation,
  SelectionGesturePhase,
};
use super::{ExportArtifact, ExportState, ScreenshotWorkspaceOutputSettings};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

/// Decode width used before the webview has reported the on-screen pane size.
const FALLBACK_TARGET_WIDTH: u32 = 1_600;
const AUTO_FIT_MOVE_EDGE: u32 = 1 << 17;
const AUTO_FIT_COMMIT_EDGE: u32 = 1 << 18;
const MINIMUM_CANVAS_SIZE: f64 = 64.0;

#[derive(Default)]
struct PreviewManager {
  has_layout: bool,
  latest_session_id: u64,
  output: Option<ScreenshotWorkspaceOutputSettings>,
  pane_target_size: Option<(u32, u32)>,
  react_output: Option<ScreenshotWorkspaceOutputSettings>,
  session_id: Option<u64>,
  sources: Vec<(u64, Arc<CapturedImage>)>,
  surface: Option<Arc<RecordingPreviewSurface>>,
  selection_gesture: Option<SelectionGestureOverride>,
  workspace_scene: Option<WorkspaceScene>,
}

struct SelectionGestureOverride {
  native_workspace_owns_presentation: bool,
  operation: SelectionGestureOperation,
  snapshot: ScreenshotWorkspaceOutputSettings,
}

fn fit_workspace_to_items(
  snapshot: &ScreenshotWorkspaceOutputSettings,
  moved_index: usize,
  moved_output: &ScreenshotOutputSettings,
) -> ScreenshotWorkspaceOutputSettings {
  let width = f64::from(snapshot.canvas.width.max(1));
  let height = f64::from(snapshot.canvas.height.max(1));
  let mut next = snapshot.clone();
  if let Some(item) = next.items.get_mut(moved_index) {
    item.output = moved_output.clone();
  }
  let mut left = 0.0_f64;
  let mut top = 0.0_f64;
  let mut right = width;
  let mut bottom = height;
  for item in &next.items {
    let output = &item.output;
    let crop_x = width * output.screenshot_crop_x_percent / 100.0;
    let crop_y = height * output.screenshot_crop_y_percent / 100.0;
    let crop_width = width * output.screenshot_crop_width_percent / 100.0;
    let crop_height = height * output.screenshot_crop_height_percent / 100.0;
    left = left.min(crop_x.floor());
    top = top.min(crop_y.floor());
    right = right.max((crop_x + crop_width).ceil());
    bottom = bottom.max((crop_y + crop_height).ceil());
  }
  let next_width = (right - left).round().max(MINIMUM_CANVAS_SIZE);
  let next_height = (bottom - top).round().max(MINIMUM_CANVAS_SIZE);
  next.canvas.width = next_width as u32;
  next.canvas.height = next_height as u32;
  for item in &mut next.items {
    let output = &mut item.output;
    let crop_x = width * output.screenshot_crop_x_percent / 100.0 - left;
    let crop_y = height * output.screenshot_crop_y_percent / 100.0 - top;
    let crop_width = width * output.screenshot_crop_width_percent / 100.0;
    let crop_height = height * output.screenshot_crop_height_percent / 100.0;
    let image_width = width * output.screenshot_image_width_percent / 100.0;
    let image_x = width * output.screenshot_image_x_percent / 100.0 - left;
    let image_y = height * output.screenshot_image_y_percent / 100.0 - top;
    output.width = next_width as u32;
    output.height = next_height as u32;
    output.screenshot_crop_x_percent = crop_x * 100.0 / next_width;
    output.screenshot_crop_y_percent = crop_y * 100.0 / next_height;
    output.screenshot_crop_width_percent = crop_width * 100.0 / next_width;
    output.screenshot_crop_height_percent = crop_height * 100.0 / next_height;
    output.screenshot_image_width_percent = image_width * 100.0 / next_width;
    output.screenshot_image_x_percent = image_x * 100.0 / next_width;
    output.screenshot_image_y_percent = image_y * 100.0 / next_height;
  }
  next
}

impl PreviewManager {
  fn require_session(&self, session_id: u64) -> Result<(), String> {
    (self.session_id == Some(session_id))
      .then_some(())
      .ok_or_else(|| "That screenshot preview session is no longer active".to_owned())
  }

  fn stop(&mut self) {
    if let Some(surface) = self.surface.as_ref() {
      surface.hide();
    }
    self.has_layout = false;
    self.output = None;
    self.pane_target_size = None;
    self.react_output = None;
    self.session_id = None;
    self.sources.clear();
    self.surface = None;
    self.selection_gesture = None;
    self.workspace_scene = None;
  }

  /// Mirrors the recording preview: composition happens at the on-screen pane
  /// size (never above the output size, never below the validation floor).
  fn scaled_output(
    pane_target_size: Option<(u32, u32)>,
    settings: &ScreenshotOutputSettings,
  ) -> ScreenshotOutputSettings {
    let (target_width, target_height) = pane_target_size
      .filter(|size| size.0 >= 16 && size.1 >= 16)
      .unwrap_or((
        FALLBACK_TARGET_WIDTH,
        ((f64::from(FALLBACK_TARGET_WIDTH) * f64::from(settings.height)
          / f64::from(settings.width.max(1)))
        .round()
        .max(1.0)) as u32,
      ));
    let factor = (f64::from(target_width) / f64::from(settings.width.max(1))).min(1.0);
    let height_factor = (f64::from(target_height) / f64::from(settings.height.max(1))).min(1.0);
    if factor >= 1.0 && height_factor >= 1.0 {
      return settings.clone();
    }
    let minimum = (64.0 / f64::from(settings.width.max(1)))
      .max(64.0 / f64::from(settings.height.max(1)))
      .min(1.0);
    let factor = factor.max(minimum);
    let height_factor = height_factor.max(minimum);
    let mut scaled = settings.clone();
    scaled.width = ((f64::from(settings.width) * factor).round().max(64.0)) as u32;
    scaled.height = ((f64::from(settings.height) * height_factor)
      .round()
      .max(64.0)) as u32;
    scaled
  }

  fn present(&self) -> Result<(), String> {
    let (Some(surface), Some(output)) = (self.surface.as_ref(), self.output.as_ref()) else {
      return Ok(());
    };
    Self::present_snapshot(surface, output, &self.sources, self.pane_target_size)
  }

  fn present_snapshot(
    surface: &RecordingPreviewSurface,
    output: &ScreenshotWorkspaceOutputSettings,
    sources: &[(u64, Arc<CapturedImage>)],
    pane_target_size: Option<(u32, u32)>,
  ) -> Result<(), String> {
    if output.canvas.width < 64 || output.canvas.height < 64 {
      return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
      let layers = output
        .items
        .iter()
        .filter_map(|item_output| {
          let (_, source) = sources.iter().find(|(id, _)| *id == item_output.id)?;
          Some((
            item_output.id,
            source.as_ref(),
            Self::scaled_output(pane_target_size, &output.output_for_id(item_output.id)),
          ))
        })
        .collect::<Vec<_>>();
      if layers.is_empty() {
        return Ok(());
      }
      surface.present_screenshot_workspace(&layers)?;
    }
    #[cfg(not(target_os = "macos"))]
    for (index, item_output) in output.items.iter().enumerate() {
      let Some((_, source)) = sources.iter().find(|(id, _)| *id == item_output.id) else {
        continue;
      };
      let scaled = Self::scaled_output(pane_target_size, &output.output_for_id(item_output.id));
      surface.present_screenshot_layer(index as u32, item_output.id, source, &scaled, index > 0)?;
    }
    Ok(())
  }

  fn present_batch(&self) -> Result<(), String> {
    let batch = self.surface.as_ref().map(|surface| surface.present_batch());
    let result = self.present();
    drop(batch);
    result
  }

  fn handle_selection_gesture(
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

#[derive(Default)]
pub struct ScreenshotPreviewState(Mutex<PreviewManager>);

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSurfacePane {
  #[allow(dead_code)]
  index: u32,
  rect: PreviewSurfaceRect,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSelectionOverlay {
  #[serde(default)]
  crop_mode: bool,
  #[serde(default)]
  image: Option<PreviewSurfaceRect>,
  layer_id: Option<u32>,
  pane_index: u32,
  radius_percent: f64,
  rect: PreviewSurfaceRect,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotPreviewTransformEvent {
  session_id: u64,
  zoom_percent: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotSelectionGestureEvent {
  delta_x: f64,
  delta_y: f64,
  edges: u32,
  operation: u32,
  pane_index: u32,
  phase: &'static str,
  scale: f64,
  session_id: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotSelectionChangeEvent {
  pane_index: Option<u32>,
  session_id: u64,
}

#[tauri::command]
pub fn start_screenshot_preview(
  app: AppHandle,
  state: tauri::State<'_, ScreenshotPreviewState>,
  artifact_id: u64,
  session_id: u64,
) -> Result<(), String> {
  let sources = {
    let export_state = app.state::<ExportState>();
    let artifact = export_state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(ExportArtifact::Screenshot { id, items, .. }) = artifact.as_ref() else {
      return Err("There is no screenshot to preview".to_owned());
    };
    if *id != artifact_id {
      return Err("That screenshot is no longer waiting to be exported".to_owned());
    }
    items
      .iter()
      .map(|item| (item.id, Arc::new(item.image.clone())))
      .collect::<Vec<_>>()
  };
  let surface = app
    .get_webview_window("export")
    .map(|window| {
      let mut surface = RecordingPreviewSurface::from_window(&window)?;
      #[cfg(any(target_os = "macos", target_os = "windows"))]
      {
        let event_app = app.clone();
        surface.enable_editor(Box::new(move |zoom_percent| {
          let _ = event_app.emit(
            "screenshot-preview://transform",
            ScreenshotPreviewTransformEvent {
              session_id,
              zoom_percent,
            },
          );
        }));
        surface.set_selection_snapping(true);
        let event_app = app.clone();
        surface.set_selection_callback(Box::new(move |pane_index| {
          let _ = event_app.emit(
            "screenshot-preview://selection-change",
            ScreenshotSelectionChangeEvent {
              pane_index,
              session_id,
            },
          );
        }));
        let event_app = app.clone();
        surface.set_selection_gesture_callback(Box::new(
          move |phase, pane_index, operation, edges, scale, delta_x, delta_y| {
            let phase_name = match &phase {
              SelectionGesturePhase::Begin => "begin",
              SelectionGesturePhase::Update => "update",
              SelectionGesturePhase::End => "end",
              SelectionGesturePhase::Cancel => "cancel",
            };
            let manager = event_app.state::<ScreenshotPreviewState>();
            // Never wait for this mutex from AppKit's main thread. Surface
            // layout commands briefly mutate the manager on a worker and then
            // synchronously marshal geometry back to AppKit; blocking here
            // would invert those locks and freeze the entire application.
            match manager.0.try_lock() {
              Ok(mut manager) => {
                let _ = manager.handle_selection_gesture(
                  phase, pane_index, operation, edges, scale, delta_x, delta_y,
                );
              }
              Err(_) if matches!(phase, SelectionGesturePhase::End) => {
                let deferred_app = event_app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                  let state = deferred_app.state::<ScreenshotPreviewState>();
                  let Ok(mut manager) = state.0.lock() else {
                    return;
                  };
                  if manager.session_id == Some(session_id) {
                    let _ = manager.handle_selection_gesture(
                      phase, pane_index, operation, edges, scale, delta_x, delta_y,
                    );
                  }
                });
              }
              Err(_) => {}
            }
            let _ = event_app.emit(
              "screenshot-preview://selection-gesture",
              ScreenshotSelectionGestureEvent {
                delta_x,
                delta_y,
                edges,
                operation: match operation {
                  SelectionGestureOperation::Move => 0,
                  SelectionGestureOperation::Resize => 1,
                  SelectionGestureOperation::Radius => 2,
                  SelectionGestureOperation::FrameResize => 3,
                  SelectionGestureOperation::FrameRadius => 4,
                  SelectionGestureOperation::CropMove => 5,
                  SelectionGestureOperation::CropResize => 6,
                },
                pane_index,
                phase: phase_name,
                scale,
                session_id,
              },
            );
          },
        ));
      }
      Ok::<_, String>(Arc::new(surface))
    })
    .transpose()?;
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  if session_id < manager.latest_session_id {
    return Ok(());
  }
  manager.stop();
  manager.latest_session_id = session_id;
  manager.session_id = Some(session_id);
  manager.sources = sources;
  manager.surface = surface;
  Ok(())
}

/// Refreshes captured sources without replacing the live Metal surface.
/// Screenshot workspaces append items while the export window stays open;
/// restarting the surface at that point can overlap an in-flight layout and
/// drawable presentation from the previous session.
#[tauri::command]
pub async fn refresh_screenshot_preview_sources(
  app: AppHandle,
  state: tauri::State<'_, ScreenshotPreviewState>,
  artifact_id: u64,
  session_id: u64,
) -> Result<(), String> {
  let existing = {
    let manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.sources.clone()
  };
  let sources = {
    let export_state = app.state::<ExportState>();
    let artifact = export_state
      .artifact
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(ExportArtifact::Screenshot { id, items, .. }) = artifact.as_ref() else {
      return Err("There is no screenshot to preview".to_owned());
    };
    if *id != artifact_id {
      return Err("That screenshot is no longer waiting to be exported".to_owned());
    }
    items
      .iter()
      .map(|item| {
        existing
          .iter()
          .find(|(id, _)| *id == item.id)
          .map(|(_, image)| (item.id, Arc::clone(image)))
          .unwrap_or_else(|| (item.id, Arc::new(item.image.clone())))
      })
      .collect::<Vec<_>>()
  };
  let presentation = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.sources = sources;
    manager.has_layout.then(|| {
      (
        manager.surface.clone(),
        manager.output.clone(),
        manager.sources.clone(),
        manager.pane_target_size,
      )
    })
  };
  if let Some((Some(surface), Some(output), sources, pane_target_size)) = presentation {
    let batch = surface.present_batch();
    PreviewManager::present_snapshot(&surface, &output, &sources, pane_target_size)?;
    drop(batch);
  }
  Ok(())
}

// Async so Tauri dispatches it off the main thread: this command blocks on a
// DirectComposition commit, and the main thread pumps the Win32 messages that
// deliver the webview's pointer input.
#[tauri::command]
pub async fn layout_screenshot_preview_surface(
  state: tauri::State<'_, ScreenshotPreviewState>,
  backdrop: Option<[f64; 4]>,
  interaction_output: ScreenshotWorkspaceOutputSettings,
  output: ScreenshotWorkspaceOutputSettings,
  panes: Vec<ScreenshotSurfacePane>,
  scale: f64,
  selection: Option<ScreenshotSelectionOverlay>,
  selection_targets: Option<Vec<ScreenshotSelectionOverlay>>,
  session_id: u64,
  viewport: PreviewSurfaceRect,
) -> Result<(), String> {
  let scale = if scale.is_finite() && scale > 0.0 {
    scale
  } else {
    1.0
  };
  let (surface, will_present, natural_size) = {
    let mut manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.react_output = Some(interaction_output.clone());
    // Pointer ownership stays native for the complete gesture. React layouts
    // may update the inspector and display-only preview model meanwhile, but
    // they cannot replace the pixel gesture snapshot until mouse-up.
    let output = if manager.selection_gesture.is_some() {
      manager.output.clone().unwrap_or(output)
    } else {
      output
    };
    let output_changed = manager.output.as_ref() != Some(&output);
    manager.output = Some(output.clone());
    if !panes.is_empty() {
      let revision = manager.workspace_scene.as_ref().map_or(0, |scene| {
        scene.revision.saturating_add(u64::from(output_changed))
      });
      manager.workspace_scene = Some(super::preview_workspace_model::screenshot_scene(
        WorldRect {
          x: viewport.x,
          y: viewport.y,
          width: viewport.width,
          height: viewport.height,
        },
        &output,
        revision,
      )?);
    }
    let mut size_changed = false;
    if let Some(pane) = panes.first() {
      let next = (
        (pane.rect.width * scale).round().max(2.0) as u32,
        (pane.rect.height * scale).round().max(2.0) as u32,
      );
      if manager.pane_target_size != Some(next) {
        manager.pane_target_size = Some(next);
        size_changed = true;
      }
    }
    let Some(surface) = manager.surface.clone() else {
      return Ok(());
    };
    let frame_owns_presentation = manager.selection_gesture.as_ref().is_some_and(|gesture| {
      gesture.operation == SelectionGestureOperation::FrameResize
        || gesture.native_workspace_owns_presentation
    });
    let will_present =
      !frame_owns_presentation && (!manager.has_layout || output_changed || size_changed);
    let natural_size = (output.canvas.width, output.canvas.height);
    manager.has_layout = true;
    (surface, will_present, natural_size)
  };
  surface.set_selection(selection.map(|overlay| PreviewSelection {
    crop_mode: u32::from(overlay.crop_mode),
    image_height: overlay.image.map_or(0.0, |image| image.height),
    image_width: overlay.image.map_or(0.0, |image| image.width),
    image_x: overlay.image.map_or(0.0, |image| image.x),
    image_y: overlay.image.map_or(0.0, |image| image.y),
    layer_id: overlay.layer_id.unwrap_or(overlay.pane_index),
    radius_disabled: 0,
    #[cfg(target_os = "macos")]
    pane_index: 0,
    #[cfg(not(target_os = "macos"))]
    pane_index: overlay.pane_index,
    x: overlay.rect.x,
    y: overlay.rect.y,
    width: overlay.rect.width,
    height: overlay.rect.height,
    radius_percent: overlay.radius_percent,
  }));
  let selection_targets = selection_targets.map(|targets| {
    targets
      .into_iter()
      .map(|target| PreviewSelection {
        crop_mode: u32::from(target.crop_mode),
        image_height: target.image.map_or(0.0, |image| image.height),
        image_width: target.image.map_or(0.0, |image| image.width),
        image_x: target.image.map_or(0.0, |image| image.x),
        image_y: target.image.map_or(0.0, |image| image.y),
        layer_id: target.layer_id.unwrap_or(target.pane_index),
        radius_disabled: 0,
        #[cfg(target_os = "macos")]
        pane_index: 0,
        #[cfg(not(target_os = "macos"))]
        pane_index: target.pane_index,
        x: target.rect.x,
        y: target.rect.y,
        width: target.rect.width,
        height: target.rect.height,
        radius_percent: target.radius_percent,
      })
      .collect::<Vec<_>>()
  });
  surface.set_selection_targets(selection_targets.as_deref());
  // Lay out first with the pane frames held back, then present: the batch
  // applies the deferred frames in the same Core Animation transaction as the
  // freshly composed drawables. Presenting before layout does not achieve
  // that - an explicit transaction opened outside any implicit one commits
  // immediately, so the drawable would land a tick before the frame and be
  // fitted into the old rect meanwhile. A layout that will not present (a
  // pure pan) applies its frames at once, or they would never land.
  surface.set_scale(scale);
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10, 1.0]));
  #[cfg(target_os = "macos")]
  if let Some(pane) = panes.first() {
    surface.layout_workspace(pane.rect, natural_size, will_present);
  }
  #[cfg(not(target_os = "macos"))]
  for pane in panes {
    surface.layout(pane.index, pane.rect, will_present);
  }
  // Open the batch before `finish_layout` so the hides, the deferred pane
  // frames and the fresh layer presents all land in one commit - on Windows
  // that is also the invoke's single compositor wait.
  let batch = will_present.then(|| surface.present_batch());
  surface.finish_layout();
  if will_present {
    // Source refresh and structural layout are separate IPC calls. Snapshot
    // only after the new pane views exist so a newly captured screenshot can
    // neither present too early nor be skipped by an older source snapshot.
    let presentation = {
      let manager = state
        .0
        .lock()
        .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
      manager.require_session(session_id)?;
      (
        manager.output.clone(),
        manager.sources.clone(),
        manager.pane_target_size,
      )
    };
    if let (Some(output), sources, pane_target_size) = presentation {
      PreviewManager::present_snapshot(&surface, &output, &sources, pane_target_size)?;
    }
  }
  drop(batch);
  Ok(())
}

#[tauri::command]
pub fn set_screenshot_preview_zoom(
  state: tauri::State<'_, ScreenshotPreviewState>,
  session_id: u64,
  zoom_percent: f64,
) -> Result<(), String> {
  if !zoom_percent.is_finite() || !(10.0..=1_600.0).contains(&zoom_percent) {
    return Err("The screenshot preview zoom is invalid".to_owned());
  }
  let surface = {
    let manager = state
      .0
      .lock()
      .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
    manager.require_session(session_id)?;
    manager.surface.clone()
  };
  if let Some(surface) = surface {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    surface.set_editor_zoom(zoom_percent);
  }
  Ok(())
}

#[tauri::command]
pub fn center_screenshot_preview_workspace(app: tauri::AppHandle) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || {
    let surface = app
      .state::<ScreenshotPreviewState>()
      .0
      .lock()
      .ok()
      .and_then(|manager| manager.surface.clone());
    if let Some(surface) = surface {
      #[cfg(any(target_os = "macos", target_os = "windows"))]
      surface.center_editor();
    }
  });
  Ok(())
}

#[tauri::command]
pub fn stop_screenshot_preview(
  state: tauri::State<'_, ScreenshotPreviewState>,
  session_id: u64,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The screenshot preview is unavailable".to_owned())?;
  if manager.session_id == Some(session_id) {
    manager.stop();
  }
  Ok(())
}
