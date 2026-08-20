// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use super::super::preview_platform::{
  RecordingPreviewSurface, SelectionGestureOperation, SelectionGesturePhase,
};
use super::super::{ExportArtifact, ExportKind, ExportState};
use super::payloads::{
  ScreenshotPreviewTransformEvent, ScreenshotSelectionChangeEvent, ScreenshotSelectionGestureEvent,
};
use super::state::ScreenshotPreviewState;

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
      .screenshot
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
    .get_webview_window(ExportKind::Screenshot.window_label().as_str())
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
