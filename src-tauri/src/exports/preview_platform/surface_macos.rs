// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! macOS preview surface: `CAMetalLayer` panes below the `WKWebView`.
//!
//! See the parent module for the contract a new platform has to satisfy. The
//! pane hierarchy, layout batching and GPU composition live in
//! `exports/recording_preview_surface_macos.m`; this file is only the FFI
//! boundary around it.

use tauri::WebviewWindow;

use super::{
  PreviewCapabilities, PreviewSelection, PreviewSurfaceRect, SelectionCallback,
  SelectionGestureCallback, SelectionGestureOperation, SelectionGesturePhase, TransformCallback,
};
use crate::exports::{media_preview, CameraOverlaySettings};
use crate::screenshots::{
  native_canvas, CapturedImage, NativeCanvas, ScreenshotOutputSettings, StillOverlay,
};

pub(super) const CAPABILITIES: PreviewCapabilities = PreviewCapabilities {
  native_workspace_editor: true,
  native_recording_preview: true,
  native_screenshot_preview: true,
};

unsafe extern "C" {
  fn screenwide_preview_surface_create(host_view: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
  fn screenwide_preview_surface_layout_workspace(
    handle: *mut std::ffi::c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    natural_width: f64,
    natural_height: f64,
    defer_draw: i32,
  );
  fn screenwide_preview_surface_layout_recording_workspace(
    handle: *mut std::ffi::c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    natural_width: f64,
    natural_height: f64,
    panes: *const NativeWorkspacePaneRect,
    pane_count: u32,
    defer_draw: i32,
  );
  fn screenwide_preview_surface_set_viewport(
    handle: *mut std::ffi::c_void,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
  );
  fn screenwide_preview_surface_begin_layout(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_finish_layout(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_begin_present(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_end_present(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_present(
    handle: *mut std::ffi::c_void,
    index: u32,
    rgba: *const u8,
    width: u32,
    height: u32,
  ) -> i32;
  fn screenwide_preview_surface_present_screenshot_workspace(
    handle: *mut std::ffi::c_void,
    layers: *const NativeWorkspaceLayer,
    layer_count: u32,
  ) -> i32;
  fn screenwide_preview_surface_present_recording_workspace(
    handle: *mut std::ffi::c_void,
    layers: *const NativeWorkspaceLayer,
    layer_count: u32,
  ) -> i32;
  fn screenwide_preview_surface_workspace_source_size(
    handle: *mut std::ffi::c_void,
    pane_index: u32,
    width: *mut u32,
    height: *mut u32,
  ) -> i32;
  fn screenwide_preview_surface_workspace_camera_source_size(
    handle: *mut std::ffi::c_void,
    pane_index: u32,
    width: *mut u32,
    height: *mut u32,
  ) -> i32;
  fn screenwide_preview_surface_update_workspace_canvas(
    handle: *mut std::ffi::c_void,
    pane_index: u32,
    canvas_width: u32,
    canvas_height: u32,
    canvas: *const NativeCanvas,
  ) -> i32;
  fn screenwide_preview_surface_update_workspace_camera_overlay(
    handle: *mut std::ffi::c_void,
    pane_index: u32,
    overlay: *const StillOverlay,
  ) -> i32;
  fn screenwide_preview_surface_redraw_workspace(handle: *mut std::ffi::c_void) -> i32;
  fn screenwide_preview_surface_hide(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_destroy(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_enable_editor(
    handle: *mut std::ffi::c_void,
    callback: Option<unsafe extern "C" fn(f64, *mut std::ffi::c_void)>,
    context: *mut std::ffi::c_void,
  );
  fn screenwide_preview_surface_set_editor_zoom(handle: *mut std::ffi::c_void, zoom_percent: f64);
  fn screenwide_preview_surface_center_editor(handle: *mut std::ffi::c_void);
  fn screenwide_preview_surface_set_selection_visible(handle: *mut std::ffi::c_void, visible: i32);
  fn screenwide_preview_surface_set_selection(
    handle: *mut std::ffi::c_void,
    selection: *const PreviewSelection,
  );
  fn screenwide_preview_surface_set_selection_targets(
    handle: *mut std::ffi::c_void,
    targets: *const PreviewSelection,
    count: usize,
    enabled: i32,
  );
  fn screenwide_preview_surface_set_selection_snapping(handle: *mut std::ffi::c_void, enabled: i32);
  fn screenwide_preview_surface_set_selection_callback(
    handle: *mut std::ffi::c_void,
    callback: Option<unsafe extern "C" fn(i32, *mut std::ffi::c_void)>,
    context: *mut std::ffi::c_void,
  );
  fn screenwide_preview_surface_set_selection_gesture_callback(
    handle: *mut std::ffi::c_void,
    callback: Option<
      unsafe extern "C" fn(u32, u32, u32, u32, f64, f64, f64, *mut std::ffi::c_void),
    >,
    context: *mut std::ffi::c_void,
  );
  fn screenwide_preview_surface_release_context_on_main(
    release: unsafe extern "C" fn(*mut std::ffi::c_void),
    context: *mut std::ffi::c_void,
  );
}

unsafe extern "C" fn release_boxed_callback<T>(context: *mut std::ffi::c_void) {
  drop(unsafe { Box::from_raw(context.cast::<T>()) });
}

/// Frees a callback box on the main thread instead of here. The native
/// callback setters apply asynchronously (a synchronous hop deadlocks against
/// the player mutex), so when this thread returns from clearing or replacing
/// a callback the main thread may still hold the old context pointer. The
/// free is queued behind that clear, which is the last block that can read it.
fn release_callback_on_main<T>(callback: Option<Box<T>>) {
  if let Some(callback) = callback {
    unsafe {
      screenwide_preview_surface_release_context_on_main(
        release_boxed_callback::<T>,
        Box::into_raw(callback).cast::<std::ffi::c_void>(),
      );
    }
  }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct NativeWorkspacePlacement {
  x: i32,
  y: i32,
  width: u32,
  height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeWorkspacePaneRect {
  index: u32,
  x: f64,
  y: f64,
  width: f64,
  height: f64,
}

#[repr(C)]
struct NativeWorkspaceLayer {
  pane_index: u32,
  layer_id: u32,
  source_rgba: *const u8,
  source_pixels: *mut std::ffi::c_void,
  source_kind: u32,
  source_token: u64,
  source_width: u32,
  source_height: u32,
  canvas_width: u32,
  canvas_height: u32,
  canvas: NativeCanvas,
  placement: NativeWorkspacePlacement,
  seconds: f64,
  cursor_rgba: *const u8,
  camera_rgba: *const u8,
  camera_pixels: *mut std::ffi::c_void,
  overlay: StillOverlay,
}

unsafe extern "C" fn transform_callback(zoom_percent: f64, context: *mut std::ffi::c_void) {
  if let Some(callback) = (context as *mut TransformCallback).as_mut() {
    callback(zoom_percent);
  }
}

unsafe extern "C" fn selection_callback(pane_index: i32, context: *mut std::ffi::c_void) {
  if let Some(callback) = (context as *mut SelectionCallback).as_mut() {
    callback(u32::try_from(pane_index).ok());
  }
}

unsafe extern "C" fn selection_gesture_callback(
  phase: u32,
  pane_index: u32,
  operation: u32,
  edges: u32,
  scale: f64,
  delta_x: f64,
  delta_y: f64,
  context: *mut std::ffi::c_void,
) {
  if let Some(callback) = (context as *mut SelectionGestureCallback).as_mut() {
    let phase = match phase {
      0 => SelectionGesturePhase::Begin,
      1 => SelectionGesturePhase::Update,
      2 => SelectionGesturePhase::End,
      3 => SelectionGesturePhase::Cancel,
      _ => return,
    };
    let operation = match operation {
      0 => SelectionGestureOperation::Move,
      1 => SelectionGestureOperation::Resize,
      2 => SelectionGestureOperation::Radius,
      3 => SelectionGestureOperation::FrameResize,
      4 => SelectionGestureOperation::FrameRadius,
      5 => SelectionGestureOperation::CropMove,
      6 => SelectionGestureOperation::CropResize,
      _ => return,
    };
    callback(phase, pane_index, operation, edges, scale, delta_x, delta_y);
  }
}

pub(crate) struct RecordingPreviewSurface {
  handle: *mut std::ffi::c_void,
  selection_callback: Option<Box<SelectionCallback>>,
  transform_callback: Option<Box<TransformCallback>>,
  selection_gesture_callback: Option<Box<SelectionGestureCallback>>,
}

/// Input for one layer in the retained recording workspace. A decoded RGBA
/// image or a native CVPixelBuffer may be supplied; optional cursor/camera
/// buffers and overlay uniforms are composed in the same Metal pass.
pub(crate) struct RecordingWorkspaceLayer<'a> {
  pub pane_index: u32,
  pub source_token: u64,
  pub source: Option<&'a CapturedImage>,
  pub source_pixels: Option<(*mut std::ffi::c_void, (u32, u32))>,
  pub settings: ScreenshotOutputSettings,
  pub placement: NativeWorkspacePlacement,
  pub seconds: f64,
  pub cursor: Option<&'a CapturedImage>,
  pub camera: Option<&'a CapturedImage>,
  pub camera_pixels: Option<(*mut std::ffi::c_void, (u32, u32))>,
  pub overlay: Option<&'a StillOverlay>,
  pub clip_cursor_at_video_edge: bool,
  pub foreground_only: bool,
}

unsafe impl Send for RecordingPreviewSurface {}
unsafe impl Sync for RecordingPreviewSurface {}

impl RecordingPreviewSurface {
  pub(crate) fn from_window(window: &WebviewWindow) -> Result<Self, String> {
    let host_view = window.ns_view().map_err(|error| error.to_string())?;
    let handle = unsafe { screenwide_preview_surface_create(host_view) };
    if handle.is_null() {
      Err("The native recording preview surface could not be created".to_owned())
    } else {
      Ok(Self {
        handle,
        selection_callback: None,
        transform_callback: None,
        selection_gesture_callback: None,
      })
    }
  }

  pub(crate) fn set_viewport(&self, rect: PreviewSurfaceRect, backdrop: [f64; 4]) {
    unsafe {
      screenwide_preview_surface_set_viewport(
        self.handle,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        backdrop[0],
        backdrop[1],
        backdrop[2],
        // The export window is transparent, so re-blending the sampled CSS
        // stack against AppKit's backing material shifts #1c1c1c to #1d1d1d.
        // Its RGB is already the final composited WebView colour.
        1.0,
      );
    }
  }

  pub(crate) fn begin_layout(&self) {
    unsafe {
      screenwide_preview_surface_begin_layout(self.handle);
    }
  }

  pub(crate) fn set_scale(&self, _scale: f64) {}

  pub(crate) fn enable_editor(&mut self, callback: TransformCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut TransformCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, Some(transform_callback), context);
    }
    release_callback_on_main(self.transform_callback.replace(callback));
  }

  pub(crate) fn set_editor_active(&self, active: bool) {
    let (callback, context) = if active {
      self
        .transform_callback
        .as_ref()
        .map_or((None, std::ptr::null_mut()), |callback| {
          (
            Some(transform_callback as unsafe extern "C" fn(f64, *mut std::ffi::c_void)),
            (&**callback) as *const TransformCallback as *mut std::ffi::c_void,
          )
        })
    } else {
      (None, std::ptr::null_mut())
    };
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, callback, context);
    }
  }

  pub(crate) fn set_editor_zoom(&self, zoom_percent: f64) {
    unsafe {
      screenwide_preview_surface_set_editor_zoom(self.handle, zoom_percent);
    }
  }

  pub(crate) fn center_editor(&self) {
    unsafe {
      screenwide_preview_surface_center_editor(self.handle);
    }
  }

  pub(crate) fn set_selection(&self, selection: Option<PreviewSelection>) {
    unsafe {
      screenwide_preview_surface_set_selection(
        self.handle,
        selection
          .as_ref()
          .map_or(std::ptr::null(), std::ptr::from_ref),
      );
    }
  }

  pub(crate) fn set_selection_visible(&self, visible: bool) {
    unsafe {
      screenwide_preview_surface_set_selection_visible(self.handle, i32::from(visible));
    }
  }

  pub(crate) fn set_selection_targets(&self, targets: Option<&[PreviewSelection]>) {
    unsafe {
      screenwide_preview_surface_set_selection_targets(
        self.handle,
        targets.map_or(std::ptr::null(), |targets| targets.as_ptr()),
        targets.map_or(0, <[PreviewSelection]>::len),
        i32::from(targets.is_some()),
      );
    }
  }

  pub(crate) fn set_selection_snapping(&self, enabled: bool) {
    unsafe {
      screenwide_preview_surface_set_selection_snapping(self.handle, i32::from(enabled));
    }
  }

  pub(crate) fn set_selection_callback(&mut self, callback: SelectionCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut SelectionCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_set_selection_callback(
        self.handle,
        Some(selection_callback),
        context,
      );
    }
    release_callback_on_main(self.selection_callback.replace(callback));
  }

  /// Installs the native selection-body gesture callback. The callback is
  /// invoked on the main thread with normalized movement from gesture start.
  /// Keeping it here, beside the existing transform callback, lets the
  /// frontend mirror the native gesture without routing pointer movement
  /// through the webview.
  pub(crate) fn set_selection_gesture_callback(&mut self, callback: SelectionGestureCallback) {
    let mut callback = Box::new(callback);
    let context = (&mut *callback) as *mut SelectionGestureCallback as *mut std::ffi::c_void;
    unsafe {
      screenwide_preview_surface_set_selection_gesture_callback(
        self.handle,
        Some(selection_gesture_callback),
        context,
      );
    }
    release_callback_on_main(self.selection_gesture_callback.replace(callback));
  }

  /// Lays out one fixed drawable over the complete viewport while retaining
  /// the logical canvas rectangle used by the native pan/zoom transform.
  pub(crate) fn layout_workspace(
    &self,
    rect: PreviewSurfaceRect,
    natural_size: (u32, u32),
    defer_draw: bool,
  ) {
    unsafe {
      screenwide_preview_surface_layout_workspace(
        self.handle,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        f64::from(natural_size.0),
        f64::from(natural_size.1),
        i32::from(defer_draw),
      );
    }
  }

  pub(crate) fn layout_recording_workspace(
    &self,
    rect: PreviewSurfaceRect,
    natural_size: (u32, u32),
    panes: &[(u32, PreviewSurfaceRect)],
    defer_draw: bool,
  ) {
    let panes = panes
      .iter()
      .map(|(index, pane)| NativeWorkspacePaneRect {
        index: *index,
        x: pane.x,
        y: pane.y,
        width: pane.width,
        height: pane.height,
      })
      .collect::<Vec<_>>();
    unsafe {
      screenwide_preview_surface_layout_recording_workspace(
        self.handle,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        f64::from(natural_size.0),
        f64::from(natural_size.1),
        panes.as_ptr(),
        panes.len().try_into().unwrap_or(u32::MAX),
        i32::from(defer_draw),
      );
    }
  }

  pub(crate) fn present(&self, index: u32, image: &CapturedImage) -> bool {
    unsafe {
      screenwide_preview_surface_present(
        self.handle,
        index,
        image.rgba.as_ptr(),
        image.width,
        image.height,
      ) != 0
    }
  }

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

  /// Composes every screenshot item into one workspace drawable and command
  /// buffer. The native presenter retains the immutable source buffers, so a
  /// later pan/zoom redraw never requires React or another source upload.
  pub(crate) fn present_screenshot_workspace(
    &self,
    layers: &[(u64, &CapturedImage, ScreenshotOutputSettings)],
  ) -> Result<bool, String> {
    let mut native_layers = Vec::with_capacity(layers.len());
    for (index, (source_token, source, settings)) in layers.iter().enumerate() {
      let mut canvas = native_canvas(source.width, source.height, settings, true)?;
      canvas.foreground_only = u32::from(index > 0);
      native_layers.push(NativeWorkspaceLayer {
        pane_index: 0,
        // Screenshot selection and gesture events address layers by their
        // workspace order. `source_token` remains the independent cache key;
        // using it as the layer identity prevents the crop magnifier from
        // resolving the selected retained source.
        layer_id: u32::try_from(index).unwrap_or(u32::MAX - 1),
        source_rgba: source.rgba.as_ptr(),
        source_pixels: std::ptr::null_mut(),
        source_kind: 0,
        source_token: *source_token,
        source_width: source.width,
        source_height: source.height,
        canvas_width: settings.width,
        canvas_height: settings.height,
        canvas,
        placement: NativeWorkspacePlacement::default(),
        seconds: 0.0,
        cursor_rgba: std::ptr::null(),
        camera_rgba: std::ptr::null(),
        camera_pixels: std::ptr::null_mut(),
        overlay: StillOverlay::default(),
      });
    }
    Ok(unsafe {
      screenwide_preview_surface_present_screenshot_workspace(
        self.handle,
        native_layers.as_ptr(),
        native_layers.len().try_into().unwrap_or(u32::MAX),
      ) != 0
    })
  }

  /// Opens a present batch: every present until the guard drops lands in one
  /// Core Animation commit together with all frames deferred by `layout`.
  /// Dropping the guard flushes even when nothing was presented, so a
  /// deferred layout never strands the panes.
  pub(crate) fn present_batch(&self) -> PresentBatch<'_> {
    unsafe {
      screenwide_preview_surface_begin_present(self.handle);
    }
    PresentBatch { surface: self }
  }

  pub(crate) fn finish_layout(&self) {
    unsafe {
      screenwide_preview_surface_finish_layout(self.handle);
    }
  }

  pub(crate) fn hide(&self) {
    unsafe {
      screenwide_preview_surface_hide(self.handle);
    }
  }
}

impl Drop for RecordingPreviewSurface {
  fn drop(&mut self) {
    unsafe {
      screenwide_preview_surface_enable_editor(self.handle, None, std::ptr::null_mut());
      screenwide_preview_surface_set_selection_callback(self.handle, None, std::ptr::null_mut());
      screenwide_preview_surface_set_selection_gesture_callback(
        self.handle,
        None,
        std::ptr::null_mut(),
      );
      screenwide_preview_surface_destroy(self.handle);
    }
    release_callback_on_main(self.transform_callback.take());
    release_callback_on_main(self.selection_callback.take());
    release_callback_on_main(self.selection_gesture_callback.take());
  }
}

#[cfg(test)]
mod tests {
  unsafe extern "C" {
    fn screenwide_gpu_still_presenter_create() -> *mut std::ffi::c_void;
    fn screenwide_gpu_still_presenter_destroy(handle: *mut std::ffi::c_void);
  }

  #[test]
  fn retained_workspace_metal_shader_compiles() {
    let presenter = unsafe { screenwide_gpu_still_presenter_create() };
    assert!(!presenter.is_null());
    unsafe { screenwide_gpu_still_presenter_destroy(presenter) };
  }
}

pub(crate) struct PresentBatch<'a> {
  surface: &'a RecordingPreviewSurface,
}

impl Drop for PresentBatch<'_> {
  fn drop(&mut self) {
    unsafe {
      screenwide_preview_surface_end_present(self.surface.handle);
    }
  }
}
