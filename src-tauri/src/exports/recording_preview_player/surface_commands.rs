// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native preview surface layout and visibility commands.

use super::*;
use crate::exports::preview_platform::workspace_editor::WorldRect;
use crate::exports::preview_platform::{PreviewSelection, PreviewSurfaceRect};
use crate::exports::preview_workspace_model::WorkspacePane;
use crate::exports::CameraOverlaySettings;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSurfacePane {
  index: u32,
  rect: PreviewSurfaceRect,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordingPreviewSelection {
  #[serde(default)]
  crop_mode: bool,
  #[serde(default)]
  image: Option<PreviewSurfaceRect>,
  layer_id: Option<u32>,
  pane_index: u32,
  radius_percent: f64,
  rect: PreviewSurfaceRect,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreviewSurfaceLayout {
  backdrop: Option<[f64; 4]>,
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  native_editor: bool,
  panes: Vec<PreviewSurfacePane>,
  recording_output: RecordingOutputSettings,
  request_id: u64,
  scale: f64,
  selection: Option<RecordingPreviewSelection>,
  selection_targets: Option<Vec<RecordingPreviewSelection>>,
  session_id: u64,
  viewport: PreviewSurfaceRect,
}

fn clear_inactive_pane_targets(targets: &mut [(u32, u32)], active: &[usize]) {
  for (index, target) in targets.iter_mut().enumerate() {
    if !active.contains(&index) {
      *target = (0, 0);
    }
  }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn recording_workspace_geometry(
  panes: &[PreviewSurfacePane],
  output: &RecordingOutputSettings,
) -> (PreviewSurfaceRect, (u32, u32)) {
  let left = panes
    .iter()
    .map(|pane| pane.rect.x)
    .fold(f64::INFINITY, f64::min);
  let top = panes
    .iter()
    .map(|pane| pane.rect.y)
    .fold(f64::INFINITY, f64::min);
  let right = panes
    .iter()
    .map(|pane| pane.rect.x + pane.rect.width)
    .fold(f64::NEG_INFINITY, f64::max);
  let bottom = panes
    .iter()
    .map(|pane| pane.rect.y + pane.rect.height)
    .fold(f64::NEG_INFINITY, f64::max);
  let bounds = PreviewSurfaceRect {
    x: left,
    y: top,
    width: (right - left).max(1.0),
    height: (bottom - top).max(1.0),
  };
  let fit = panes
    .iter()
    .find_map(|pane| {
      let width = if pane.index == 0 {
        output.primary.width
      } else {
        output.camera.width
      };
      (width > 0 && pane.rect.width > 0.0).then_some(pane.rect.width / f64::from(width))
    })
    .unwrap_or(1.0)
    .max(f64::EPSILON);
  (
    bounds,
    (
      (bounds.width / fit).round().max(1.0) as u32,
      (bounds.height / fit).round().max(1.0) as u32,
    ),
  )
}

// Async so Tauri dispatches it off the main thread: this command blocks on a
// DirectComposition commit, and the main thread pumps the Win32 messages that
// deliver the webview's pointer input - blocking it there starves the very
// drag this layout is following.
#[tauri::command]
pub async fn layout_recording_preview_surface(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  layout: RecordingPreviewSurfaceLayout,
) -> Result<(), String> {
  let RecordingPreviewSurfaceLayout {
    backdrop,
    bake_camera,
    camera_overlay,
    native_editor,
    panes,
    recording_output,
    request_id,
    scale,
    selection,
    selection_targets,
    session_id,
    viewport,
  } = layout;
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if request_id < manager.latest_layout_request {
    return Ok(());
  }
  manager.latest_layout_request = request_id;
  let settings = manager
    .sources
    .as_ref()
    .ok_or_else(|| "The recording preview player is not open".to_owned())?
    .composition_settings
    .clone()
    .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?;
  let (composition_changed, bake_changed) = {
    let current = settings
      .read()
      .map_err(|_| "The recording preview composition is unavailable".to_owned())?;
    (
      current.bake_camera != bake_camera
        || current.camera_overlay != camera_overlay
        || current.recording_output != recording_output,
      current.bake_camera != bake_camera,
    )
  };
  *settings
    .write()
    .map_err(|_| "The recording preview composition is unavailable".to_owned())? =
    PreviewCompositionSettings {
      bake_camera,
      camera_overlay: camera_overlay.clone(),
      recording_output: recording_output.clone(),
    };
  let scale = if scale.is_finite() && scale > 0.0 {
    scale
  } else {
    1.0
  };
  let active_pane_indices = panes
    .iter()
    .map(|pane| pane.index as usize)
    .collect::<Vec<_>>();
  clear_inactive_pane_targets(&mut manager.pane_target_sizes, &active_pane_indices);
  let mut sizes_changed = false;
  let needs_initial_frame = panes.iter().any(|pane| {
    manager
      .pane_target_sizes
      .get(pane.index as usize)
      .is_none_or(|size| *size == (0, 0))
  });
  for pane in &panes {
    let index = pane.index as usize;
    if manager.pane_target_sizes.len() <= index {
      manager.pane_target_sizes.resize(index + 1, (0, 0));
      sizes_changed = true;
    }
    let next = if manager.full_resolution {
      let output = if index == 0 {
        &recording_output.primary
      } else {
        &recording_output.camera
      };
      (output.width, output.height)
    } else {
      (
        (pane.rect.width * scale).round().max(2.0) as u32,
        (pane.rect.height * scale).round().max(2.0) as u32,
      )
    };
    if manager.pane_target_sizes[index] != next {
      manager.pane_target_sizes[index] = next;
      sizes_changed = true;
    }
  }
  if !panes.is_empty() {
    let revision = manager.workspace_scene.as_ref().map_or(0, |scene| {
      scene
        .revision
        .saturating_add(u64::from(composition_changed))
    });
    let workspace_panes = panes
      .iter()
      .map(|pane| WorkspacePane {
        id: pane.index,
        rect: WorldRect {
          x: pane.rect.x,
          y: pane.rect.y,
          width: pane.rect.width,
          height: pane.rect.height,
        },
      })
      .collect::<Vec<_>>();
    manager.workspace_scene = Some(crate::exports::preview_workspace_model::recording_scene(
      WorldRect {
        x: viewport.x,
        y: viewport.y,
        width: viewport.width,
        height: viewport.height,
      },
      &workspace_panes,
      bake_camera,
      camera_overlay.clone(),
      &recording_output,
      revision,
    )?);
  }
  let Some(surface) = manager
    .sources
    .as_ref()
    .and_then(|sources| sources.preview_surface.as_ref())
  else {
    return Ok(());
  };
  #[cfg(target_os = "macos")]
  let retained_recomposition = if composition_changed && !bake_changed {
    let active_outputs = panes
      .iter()
      .filter(|pane| !bake_camera || pane.index == 0)
      .map(|pane| {
        (
          pane.index,
          if pane.index == 0 {
            &recording_output.primary
          } else {
            &recording_output.camera
          },
        )
      })
      .collect::<Vec<_>>();
    surface.recompose_recording_workspace(
      &active_outputs,
      bake_camera.then_some((
        camera_overlay,
        recording_output.camera.drop_shadow,
        recording_output.camera_on_top,
      )),
    )?
  } else {
    false
  };
  #[cfg(not(target_os = "macos"))]
  let retained_recomposition = false;
  let wants_still = (sizes_changed || composition_changed)
    && !manager.is_playing
    && manager.still_decoder.is_some();
  // The decoder can produce its first still before the DOM has supplied a
  // native pane, in which case there is nowhere to present it. Ask for that
  // initial frame again once the first real layout exists. A bake toggle
  // also needs the decoder: the newly active mode's source cache is absent
  // or stale. Every other Windows change redraws synchronously from the
  // cached sources below - the decoder only ever supplies frames.
  let needs_decoder_still = wants_still
    && !retained_recomposition
    && (!cfg!(target_os = "windows") || needs_initial_frame || bake_changed);
  let redraw_still = cfg!(target_os = "windows") && wants_still && !needs_decoder_still;
  // A present is on its way (re-composed still, or live playback frames), so
  // the pane may hold its size until that frame lands rather than fitting the
  // previous drawable into the new rect for a display tick.
  let defer_resize = needs_decoder_still || redraw_still || manager.is_playing;
  surface.set_scale(scale);
  surface.set_selection(selection.map(|selection| PreviewSelection {
    crop_mode: u32::from(selection.crop_mode),
    image_height: selection.image.map_or(0.0, |image| image.height),
    image_width: selection.image.map_or(0.0, |image| image.width),
    image_x: selection.image.map_or(0.0, |image| image.x),
    image_y: selection.image.map_or(0.0, |image| image.y),
    layer_id: selection.layer_id.unwrap_or(selection.pane_index),
    radius_disabled: u32::from(selection.layer_id == Some(u32::MAX)),
    pane_index: selection.pane_index,
    x: selection.rect.x,
    y: selection.rect.y,
    width: selection.rect.width,
    height: selection.rect.height,
    radius_percent: selection.radius_percent,
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
        radius_disabled: u32::from(target.layer_id == Some(u32::MAX)),
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
  surface.set_editor_active(native_editor);
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10, 1.0]));
  #[cfg(any(target_os = "macos", target_os = "windows"))]
  {
    if !panes.is_empty() {
      let (workspace, natural_size) = recording_workspace_geometry(&panes, &recording_output);
      let pane_rects = panes
        .iter()
        .map(|pane| (pane.index, pane.rect))
        .collect::<Vec<_>>();
      surface.layout_recording_workspace(workspace, natural_size, &pane_rects, defer_resize);
    }
  }
  #[cfg(not(any(target_os = "macos", target_os = "windows")))]
  for pane in panes {
    surface.layout(pane.index, pane.rect, defer_resize);
  }
  // One commit for the whole invoke: with the batch already open,
  // `finish_layout` leaves its commit to the flush after the redraw, so the
  // pane geometry and the re-composed still cost a single compositor wait.
  // The decoder path keeps the batch closed - its present arrives later and
  // must find the geometry still parked.
  #[cfg(target_os = "windows")]
  let layout_batch = redraw_still.then(|| surface.present_batch());
  #[cfg(target_os = "macos")]
  if retained_recomposition {
    surface.redraw_recording_workspace();
  }
  surface.finish_layout();
  #[cfg(target_os = "windows")]
  let has_camera = manager
    .sources
    .as_ref()
    .is_some_and(|sources| sources.camera_path.is_some());
  #[cfg(target_os = "windows")]
  let redraw_failed = redraw_still
    && !surface
      .redraw_still(
        bake_camera && has_camera,
        &recording_output.primary,
        &recording_output.camera,
        camera_overlay,
        recording_output.camera.drop_shadow,
        recording_output.camera_on_top,
      )
      .unwrap_or(false);
  #[cfg(target_os = "windows")]
  drop(layout_batch);
  #[cfg(not(target_os = "windows"))]
  let redraw_failed = redraw_still;
  if needs_decoder_still || redraw_failed {
    if let Some(decoder) = &manager.still_decoder {
      decoder.seek(
        manager.position_ms,
        manager.latest_seek_request,
        false,
        manager.pane_target_sizes.clone(),
      )?;
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::clear_inactive_pane_targets;

  #[test]
  fn reenabled_panes_require_a_fresh_present_even_at_full_resolution() {
    let mut targets = [(3_840, 2_160), (1_920, 1_080)];
    clear_inactive_pane_targets(&mut targets, &[0]);
    assert_eq!(targets, [(3_840, 2_160), (0, 0)]);

    let camera_needs_present = targets[1] == (0, 0);
    assert!(camera_needs_present);
  }
}

#[tauri::command]
pub fn set_recording_preview_zoom(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  session_id: u64,
  zoom_percent: f64,
) -> Result<(), String> {
  if !zoom_percent.is_finite() || !(10.0..=1_600.0).contains(&zoom_percent) {
    return Err("The recording preview zoom is invalid".to_owned());
  }
  let manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if let Some(surface) = manager
    .sources
    .as_ref()
    .and_then(|sources| sources.preview_surface.as_ref())
  {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    surface.set_editor_zoom(zoom_percent);
  }
  Ok(())
}

#[tauri::command]
pub fn center_recording_preview_workspace(app: tauri::AppHandle) -> Result<(), String> {
  tauri::async_runtime::spawn_blocking(move || {
    let surface = {
      let state = app.state::<RecordingPreviewPlayerState>();
      state.0.lock().ok().and_then(|manager| {
        manager
          .sources
          .as_ref()
          .and_then(|sources| sources.preview_surface.clone())
      })
    };
    if let Some(surface) = surface {
      #[cfg(any(target_os = "macos", target_os = "windows"))]
      surface.center_editor();
    }
  });
  Ok(())
}
