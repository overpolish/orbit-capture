// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(target_os = "macos")]
use super::worker::PlaybackMode;
use super::{PreviewCompositionSettings, PreviewPlayerManager};

impl PreviewPlayerManager {
  pub(super) fn selection_composition(&self) -> Option<PreviewCompositionSettings> {
    self
      .sources
      .as_ref()?
      .composition_settings
      .as_ref()?
      .read()
      .ok()
      .map(|settings| settings.clone())
  }

  /// Recomposes the paused stills from the cached full-resolution sources
  /// against whatever composition is current, the way macOS recomposes its
  /// retained workspace. `Ok(false)` means a source is not cached yet and the
  /// decoder has to supply the frame - `redraw_still` still flushes its
  /// present batch on that path, so geometry a deferred layout parked reaches
  /// the compositor instead of stranding the pane at its previous box.
  #[cfg(target_os = "windows")]
  pub(super) fn redraw_still_now(&self) -> Result<bool, String> {
    let sources = self
      .sources
      .as_ref()
      .ok_or_else(|| "The recording preview player is not open".to_owned())?;
    let surface = sources
      .preview_surface
      .as_ref()
      .ok_or_else(|| "The recording preview surface is unavailable".to_owned())?;
    let composition = sources
      .composition_settings
      .as_ref()
      .ok_or_else(|| "The recording preview composition is unavailable".to_owned())?
      .read()
      .map_err(|_| "The recording preview composition is unavailable".to_owned())?
      .clone();
    surface.redraw_still(
      composition.bake_camera && sources.camera_path.is_some(),
      &composition.recording_output.primary,
      &composition.recording_output.camera,
      composition.camera_overlay,
      composition.recording_output.camera.drop_shadow,
      composition.recording_output.camera_on_top,
    )
  }

  #[cfg(target_os = "windows")]
  pub(super) fn refresh_selection_preview(&self, _layer_id: u32) -> Result<(), String> {
    self.redraw_still_now().map(|_| ())
  }

  #[cfg(target_os = "macos")]
  pub(super) fn refresh_selection_preview(&mut self, layer_id: u32) -> Result<(), String> {
    let retained = self
      .sources
      .as_ref()
      .and_then(|sources| {
        let surface = sources.preview_surface.as_ref()?;
        let composition = sources.composition_settings.as_ref()?.read().ok()?.clone();
        if layer_id != 1 {
          return None;
        }
        let mut panes = vec![(0, &composition.recording_output.primary)];
        if !composition.bake_camera && sources.camera_path.is_some() {
          panes.push((1, &composition.recording_output.camera));
        }
        surface
          .recompose_recording_workspace(
            &panes,
            composition.bake_camera.then_some((
              composition.camera_overlay,
              composition.recording_output.camera.drop_shadow,
              composition.recording_output.camera_on_top,
            )),
          )
          .ok()
          .filter(|updated| *updated)
          .map(|_| surface.redraw_recording_workspace())
      })
      .unwrap_or(false);
    if retained {
      return Ok(());
    }
    self.restart(PlaybackMode::InteractiveStill)
  }
}
