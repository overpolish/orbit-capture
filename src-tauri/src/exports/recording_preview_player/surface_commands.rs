// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Native preview surface layout and visibility commands.

use super::*;
use crate::exports::preview_platform::PreviewSurfaceRect;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSurfacePane {
  index: u32,
  rect: PreviewSurfaceRect,
}

#[tauri::command]
pub fn layout_recording_preview_surface(
  state: tauri::State<'_, RecordingPreviewPlayerState>,
  backdrop: Option<[f64; 3]>,
  panes: Vec<PreviewSurfacePane>,
  request_id: u64,
  scale: f64,
  session_id: u64,
  viewport: PreviewSurfaceRect,
) -> Result<(), String> {
  let mut manager = state
    .0
    .lock()
    .map_err(|_| "The recording preview player is unavailable".to_owned())?;
  manager.require_session(session_id)?;
  if request_id < manager.latest_layout_request {
    return Ok(());
  }
  manager.latest_layout_request = request_id;
  let scale = if scale.is_finite() && scale > 0.0 {
    scale
  } else {
    1.0
  };
  let mut sizes_changed = false;
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
  surface.begin_layout();
  surface.set_viewport(viewport, backdrop.unwrap_or([0.09, 0.09, 0.10]));
  for pane in panes {
    surface.layout(pane.index, pane.rect);
  }
  surface.finish_layout();
  // Re-presenting is only needed when the panes changed size (zoom, resize);
  // pure pans just move the native views over the existing drawables.
  if sizes_changed && !manager.is_playing {
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
