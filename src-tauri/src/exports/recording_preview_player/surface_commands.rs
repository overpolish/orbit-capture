// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native preview surface layout and visibility commands.

use super::*;
use crate::exports::preview_platform::PreviewSurfaceRect;
use crate::exports::CameraOverlaySettings;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSurfacePane {
  index: u32,
  rect: PreviewSurfaceRect,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingPreviewSurfaceLayout {
  backdrop: Option<[f64; 4]>,
  bake_camera: bool,
  camera_overlay: CameraOverlaySettings,
  panes: Vec<PreviewSurfacePane>,
  recording_output: RecordingOutputSettings,
  request_id: u64,
  scale: f64,
  session_id: u64,
  viewport: PreviewSurfaceRect,
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
    panes,
    recording_output,
    request_id,
    scale,
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
    let next = (
      (pane.rect.width * scale).round().max(2.0) as u32,
      (pane.rect.height * scale).round().max(2.0) as u32,
    );
    if manager.pane_target_sizes[index] != next {
      manager.pane_target_sizes[index] = next;
      sizes_changed = true;
    }
  }
  let Some(surface) = manager
    .sources
    .as_ref()
    .and_then(|sources| sources.preview_surface.as_ref())
  else {
    return Ok(());
  };
  let wants_still =
    (sizes_changed || composition_changed) && !manager.is_playing && manager.still_decoder.is_some();
  // The decoder can produce its first still before the DOM has supplied a
  // native pane, in which case there is nowhere to present it. Ask for that
  // initial frame again once the first real layout exists. A bake toggle
  // also needs the decoder: the newly active mode's source cache is absent
  // or stale. Every other Windows change redraws synchronously from the
  // cached sources below - the decoder only ever supplies frames.
  let needs_decoder_still =
    wants_still && (!cfg!(target_os = "windows") || needs_initial_frame || bake_changed);
  let redraw_still = cfg!(target_os = "windows") && wants_still && !needs_decoder_still;
  // A present is on its way (re-composed still, or live playback frames), so
  // the pane may hold its size until that frame lands rather than fitting the
  // previous drawable into the new rect for a display tick.
  let defer_resize = needs_decoder_still || redraw_still || manager.is_playing;
  surface.set_scale(scale);
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10, 1.0]));
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
