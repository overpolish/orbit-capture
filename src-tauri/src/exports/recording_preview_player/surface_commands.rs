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

#[tauri::command]
pub fn layout_recording_preview_surface(
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
  let composition_changed = {
    let current = settings
      .read()
      .map_err(|_| "The recording preview composition is unavailable".to_owned())?;
    current.bake_camera != bake_camera
      || current.camera_overlay != camera_overlay
      || current.recording_output != recording_output
  };
  *settings
    .write()
    .map_err(|_| "The recording preview composition is unavailable".to_owned())? =
    PreviewCompositionSettings {
      bake_camera,
      camera_overlay,
      recording_output,
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
  // The decoder can produce its first still before the DOM has supplied a
  // native pane, in which case there is nowhere to present it. Ask for that
  // initial frame again once the first real layout exists. Later Windows
  // zooms keep the cached full-resolution swap chain and need no re-decode.
  let recompose_still = (sizes_changed || composition_changed)
    && !manager.is_playing
    && manager.still_decoder.is_some()
    && (!cfg!(target_os = "windows") || needs_initial_frame || composition_changed);
  // A present is on its way (re-composed still, or live playback frames), so
  // the pane may hold its size until that frame lands rather than fitting the
  // previous drawable into the new rect for a display tick.
  let defer_resize = recompose_still || manager.is_playing;
  surface.set_scale(scale);
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10, 1.0]));
  for pane in panes {
    surface.layout(pane.index, pane.rect, defer_resize);
  }
  surface.finish_layout();
  if recompose_still {
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
