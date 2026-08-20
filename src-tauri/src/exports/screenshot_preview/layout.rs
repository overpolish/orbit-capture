// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use tauri::AppHandle;

use super::super::preview_platform::workspace_editor::WorldRect;
#[cfg(target_os = "macos")]
use super::super::preview_platform::SelectionGestureOperation;
use super::super::preview_platform::{PreviewSelection, PreviewSurfaceRect};
use super::super::ScreenshotWorkspaceOutputSettings;
use super::payloads::{ScreenshotSelectionOverlay, ScreenshotSurfacePane};
use super::state::{PreviewManager, ScreenshotPreviewState};

// Async so Tauri dispatches it off the main thread: this command blocks on a
// DirectComposition commit, and the main thread pumps the Win32 messages that
// deliver the webview's pointer input.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn layout_screenshot_preview_surface(
  app: AppHandle,
  state: tauri::State<'_, ScreenshotPreviewState>,
  backdrop: Option<[f64; 4]>,
  interaction_output: ScreenshotWorkspaceOutputSettings,
  // The native interaction view sits above the webview, so it swallows clicks
  // on DOM controls painted over the viewport (the save overlay's Cancel
  // button). React turns the editor off for the duration of a save and back on
  // afterwards; every other layout leaves it on, which is the old behaviour.
  native_editor: Option<bool>,
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
    // macOS: the retained GPU workspace presents a live Frame resize or
    // auto-fit Move itself, so a layout must not race it with an equivalent
    // but differently normalised scene. Windows has no such presenter: the
    // layout re-presents the manager's own gesture output (never React's -
    // see above), which also covers a gesture sample whose present was
    // dropped on a contended lock, the way the recording layout redraws its
    // still.
    #[cfg(target_os = "macos")]
    let frame_owns_presentation = manager.selection_gesture.as_ref().is_some_and(|gesture| {
      gesture.operation == SelectionGestureOperation::FrameResize
        || gesture.native_workspace_owns_presentation
    });
    #[cfg(not(target_os = "macos"))]
    let frame_owns_presentation = false;
    let will_present =
      !frame_owns_presentation && (!manager.has_layout || output_changed || size_changed);
    let natural_size = (output.canvas.width, output.canvas.height);
    manager.has_layout = true;
    (surface, will_present, natural_size)
  };
  // The natural canvas size only drives the retained macOS workspace layout;
  // the Windows path lays panes out from their own rects.
  #[cfg(not(target_os = "macos"))]
  let _ = natural_size;
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
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  surface.set_editor_active(native_editor.unwrap_or(true));
  // No interaction view exists off the two native preview backends.
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  let _ = native_editor;
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
      (manager.output.clone(), manager.sources.clone())
    };
    if let (Some(output), sources) = presentation {
      let staged = PreviewManager::present_snapshot(&surface, &output, &sources)?;
      if !staged {
        PreviewManager::present_once_pane_exists(&app, session_id, 0);
      }
    }
  }
  drop(batch);
  Ok(())
}
