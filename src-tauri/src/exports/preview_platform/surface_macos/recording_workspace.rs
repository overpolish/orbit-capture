// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::ffi::{
  screenwide_preview_surface_present_recording_workspace,
  screenwide_preview_surface_redraw_workspace,
  screenwide_preview_surface_update_workspace_camera_overlay,
  screenwide_preview_surface_update_workspace_canvas,
  screenwide_preview_surface_workspace_camera_source_size,
  screenwide_preview_surface_workspace_source_size,
};
use super::native_types::{NativeWorkspaceLayer, RecordingWorkspaceLayer};
use super::RecordingPreviewSurface;
use crate::exports::{media_preview, CameraOverlaySettings};
use crate::screenshots::{native_canvas, ScreenshotOutputSettings, StillOverlay};

impl RecordingPreviewSurface {
  /// Presents a retained recording scene with explicit per-layer placements.
  /// Unlike screenshot layers, recording panes are not implicitly coincident.
  pub(crate) fn present_recording_workspace(
    &self,
    layers: &[RecordingWorkspaceLayer<'_>],
  ) -> Result<bool, String> {
    let mut native_layers = Vec::with_capacity(layers.len());
    for layer in layers {
      let (source_width, source_height, source_rgba, source_pixels, source_kind) =
        if let Some(source) = layer.source {
          (
            source.width,
            source.height,
            source.rgba.as_ptr(),
            std::ptr::null_mut(),
            0,
          )
        } else if let Some((pixels, size)) = layer.source_pixels {
          (size.0, size.1, std::ptr::null(), pixels, 1)
        } else {
          return Err("Recording workspace layer has no source".to_owned());
        };
      let mut canvas = native_canvas(source_width, source_height, &layer.settings, true)?;
      canvas.clip_cursor_at_video_edge = u32::from(layer.clip_cursor_at_video_edge);
      canvas.foreground_only = u32::from(layer.foreground_only);
      let mut overlay = layer
        .overlay
        .map_or_else(StillOverlay::default, |overlay| unsafe {
          std::ptr::read(overlay)
        });
      let (cursor_rgba, cursor_dims) = layer.cursor.map_or((std::ptr::null(), (0, 0)), |cursor| {
        (cursor.rgba.as_ptr(), (cursor.width, cursor.height))
      });
      let (camera_rgba, camera_dims) = layer.camera.map_or((std::ptr::null(), (0, 0)), |camera| {
        (camera.rgba.as_ptr(), (camera.width, camera.height))
      });
      let (camera_pixels, camera_pixel_dims) = layer
        .camera_pixels
        .map_or((std::ptr::null_mut(), (0, 0)), |(pixels, size)| {
          (pixels, size)
        });
      if overlay.cursor_source_width == 0 {
        overlay.cursor_source_width = cursor_dims.0;
      }
      if overlay.cursor_source_height == 0 {
        overlay.cursor_source_height = cursor_dims.1;
      }
      if overlay.camera_source_width == 0 {
        overlay.camera_source_width = camera_dims.0;
      }
      if overlay.camera_source_height == 0 {
        overlay.camera_source_height = camera_dims.1;
      }
      if overlay.camera_source_width == 0 {
        overlay.camera_source_width = camera_pixel_dims.0;
      }
      if overlay.camera_source_height == 0 {
        overlay.camera_source_height = camera_pixel_dims.1;
      }
      native_layers.push(NativeWorkspaceLayer {
        pane_index: layer.pane_index,
        layer_id: layer.pane_index,
        source_rgba,
        source_pixels,
        source_kind,
        source_token: layer.source_token,
        source_width,
        source_height,
        canvas_width: layer.settings.width,
        canvas_height: layer.settings.height,
        canvas,
        placement: layer.placement,
        seconds: layer.seconds,
        cursor_rgba,
        camera_rgba,
        camera_pixels,
        overlay,
      });
    }
    Ok(unsafe {
      screenwide_preview_surface_present_recording_workspace(
        self.handle,
        native_layers.as_ptr(),
        native_layers.len().try_into().unwrap_or(u32::MAX),
      ) != 0
    })
  }

  /// Rebuilds retained layer uniforms against the already resident GPU source
  /// buffers. This keeps crop/output transitions in the same native draw as
  /// the OSC without asking the still decoder for identical source pixels.
  pub(crate) fn recompose_recording_workspace(
    &self,
    panes: &[(u32, &ScreenshotOutputSettings)],
    baked_camera: Option<(CameraOverlaySettings, bool, bool)>,
  ) -> Result<bool, String> {
    let mut updates = Vec::with_capacity(panes.len());
    let mut preview_sizes = Vec::with_capacity(panes.len());
    for (pane_index, settings) in panes {
      let mut source_width = 0;
      let mut source_height = 0;
      let source_found = unsafe {
        screenwide_preview_surface_workspace_source_size(
          self.handle,
          *pane_index,
          &mut source_width,
          &mut source_height,
        ) != 0
      };
      if !source_found {
        return Ok(false);
      }
      // The retained layer can still contain the pre-undo frame dimensions.
      // Reusing those dimensions updates the crop uniforms but stretches the
      // restored pixels into the stale canvas until another native gesture
      // happens to resize it. The incoming settings are the semantic source
      // of truth once there is no active native gesture, so update the canvas
      // dimensions and uniforms together.
      let canvas_width = settings.width;
      let canvas_height = settings.height;
      let preview_settings = (*settings).clone();
      preview_sizes.push((*pane_index, canvas_width, canvas_height));
      updates.push((
        *pane_index,
        canvas_width,
        canvas_height,
        native_canvas(source_width, source_height, &preview_settings, true)?,
      ));
    }
    for (pane_index, width, height, canvas) in updates {
      let updated = unsafe {
        screenwide_preview_surface_update_workspace_canvas(
          self.handle,
          pane_index,
          width,
          height,
          &canvas,
        ) != 0
      };
      if !updated {
        return Ok(false);
      }
    }
    if let Some((settings, drop_shadow, camera_on_top)) = baked_camera {
      let mut camera_width = 0;
      let mut camera_height = 0;
      let found = unsafe {
        screenwide_preview_surface_workspace_camera_source_size(
          self.handle,
          0,
          &mut camera_width,
          &mut camera_height,
        ) != 0
      };
      let Some((_, screen_width, screen_height)) =
        preview_sizes.iter().find(|(index, _, _)| *index == 0)
      else {
        return Ok(false);
      };
      if !found {
        return Ok(false);
      }
      let geometry = media_preview::bake_geometry(media_preview::BakedVideoExportOptions {
        camera_drop_shadow: drop_shadow,
        camera_height,
        camera_width,
        overlay: settings,
        screen_height: *screen_height,
        screen_width: *screen_width,
        video: media_preview::VideoExportOptions {
          compression: 0,
          resolution_scale_percent: 100,
          source_scale_percent: 100,
        },
      })?;
      let overlay = StillOverlay {
        camera_crop_x: geometry.crop_x,
        camera_crop_y: geometry.crop_y,
        camera_crop_width: geometry.crop_width,
        camera_crop_height: geometry.crop_height,
        camera_frame_x: geometry.frame_x,
        camera_frame_y: geometry.frame_y,
        camera_frame_width: geometry.frame_width,
        camera_frame_height: geometry.frame_height,
        camera_radius: geometry.radius,
        camera_source_width: camera_width,
        camera_source_height: camera_height,
        camera_drop_shadow: u32::from(drop_shadow),
        camera_on_top: u32::from(camera_on_top),
        ..StillOverlay::default()
      };
      let updated = unsafe {
        screenwide_preview_surface_update_workspace_camera_overlay(self.handle, 0, &overlay) != 0
      };
      if !updated {
        return Ok(false);
      }
    }
    Ok(true)
  }

  /// Presents the retained recording sources after a uniform-only edit.
  pub(crate) fn redraw_recording_workspace(&self) -> bool {
    unsafe { screenwide_preview_surface_redraw_workspace(self.handle) != 0 }
  }
}
