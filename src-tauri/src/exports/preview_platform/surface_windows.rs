// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows preview surface: GPU frames presented by DirectComposition beneath
//! WebView2's child window. Media Foundation and this surface share one D3D11 device, so live
//! recording frames never enter system memory or cross Tauri IPC, while transparent webview
//! regions leave DOM controls above the video.

use std::sync::{
  atomic::{AtomicU32, Ordering},
  Mutex, OnceLock,
};

use tauri::WebviewWindow;
use windows::{
  core::Interface,
  Win32::{
    Foundation::{HMODULE, HWND},
    Graphics::{
      Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1},
      Direct3D10::ID3D10Multithread,
      Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
        ID3D11Resource, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ,
        D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
        D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
      },
      DirectComposition::{
        DCompositionCreateDevice, IDCompositionDevice, IDCompositionRectangleClip,
        IDCompositionScaleTransform, IDCompositionTarget, IDCompositionVisual,
      },
      Dxgi::{
        Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
        IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGISwapChain3, DXGI_PRESENT,
        DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
        DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
      },
    },
  },
};

#[path = "surface_windows/compositor.rs"]
mod compositor;
#[path = "surface_windows/window.rs"]
mod window;

use super::{PreviewCapabilities, PreviewSurfaceRect};
use crate::exports::media_preview::{BakeGeometry, BakedVideoExportOptions, VideoExportOptions};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

pub(super) const CAPABILITIES: PreviewCapabilities = PreviewCapabilities {
  native_recording_preview: true,
  native_screenshot_preview: true,
};

pub(crate) struct StillOverlay;

#[derive(Clone, Copy)]
pub(crate) struct ComposedFrame {
  pub cursor: Option<crate::exports::cursor_effects::GpuCursor>,
  pub foreground_only: bool,
  pub seconds: f64,
}

type ClipboardCamera<'a> = (
  &'a ID3D11Texture2D,
  u32,
  (u32, u32),
  BakeGeometry,
  bool,
  bool,
);

struct Gpu {
  backdrop: Backdrop,
  compositor: compositor::Compositor,
  composition: IDCompositionDevice,
  context: ID3D11DeviceContext,
  device: ID3D11Device,
  factory: IDXGIFactory2,
  root: IDCompositionVisual,
  _target: IDCompositionTarget,
}

struct Backdrop {
  scale_transform: IDCompositionScaleTransform,
  swap_chain: IDXGISwapChain3,
  visual: IDCompositionVisual,
}

struct Pane {
  /// Allocated swap-chain size. Grown with headroom and kept, so an
  /// interactive canvas resize does not reallocate GPU buffers per pointer
  /// move; `SetSourceSize` presents only the `content_size` region.
  buffer_size: (u32, u32),
  clip: IDCompositionRectangleClip,
  clip_edges: (i32, i32, i32, i32),
  /// The presented region of the buffer - the actual output resolution.
  content_size: (u32, u32),
  display_size: (i32, i32),
  /// Geometry laid out but not yet committed. Applying it before the matching
  /// frame is composed would show the previous buffer fitted into the new
  /// rect for a display tick; the next present (or batch flush) publishes it
  /// together with the new pixels.
  pending_geometry: bool,
  /// The cursor and timeline state of the last composed frame. A paused
  /// output-settings change redraws from the cached source with this
  /// composition instead of round-tripping through the decoder.
  last_composition: Option<ComposedFrame>,
  /// A frame drawn inside an open present batch, waiting for the batch flush
  /// to call `Present` so every pane's new pixels reach the compositor in the
  /// same pass.
  pending_present: bool,
  position: (i32, i32),
  scale_transform: IDCompositionScaleTransform,
  seen: bool,
  source: Option<compositor::SourceTexture>,
  source_token: Option<u64>,
  swap_chain: IDXGISwapChain3,
  visual: IDCompositionVisual,
}

struct SurfaceState {
  backdrop: [f64; 4],
  camera_source: Option<compositor::SourceTexture>,
  panes: Vec<Option<Pane>>,
  primary_composition: Option<ComposedFrame>,
  scale: f64,
  viewport: PreviewSurfaceRect,
}

struct SurfaceInner {
  /// Open `present_batch` guards. While positive, presents park their frames
  /// on the pane and the closing guard publishes every frame and every
  /// pending geometry in one flush (the DirectComposition analogue of the
  /// macOS single-`CATransaction` batch).
  batch_depth: AtomicU32,
  gpu: Gpu,
  state: Mutex<SurfaceState>,
}

pub(crate) struct RecordingPreviewSurface {
  inner: std::sync::Arc<SurfaceInner>,
}

/// An offscreen instance of the live preview compositor. Its source and target
/// textures are allocated once and reused for every exported frame.
pub(crate) struct WindowsExportCompositor {
  camera: Option<compositor::SourceTexture>,
  inner: std::sync::Arc<SurfaceInner>,
  output_size: (u32, u32),
  source: compositor::SourceTexture,
}

static PREVIEW_SURFACE: OnceLock<Result<std::sync::Arc<SurfaceInner>, String>> = OnceLock::new();

unsafe impl Send for RecordingPreviewSurface {}
unsafe impl Sync for RecordingPreviewSurface {}
unsafe impl Send for SurfaceInner {}
unsafe impl Sync for SurfaceInner {}

impl Gpu {
  fn new(host: HWND) -> Result<Self, String> {
    let mut device = None;
    let mut context = None;
    unsafe {
      D3D11CreateDevice(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
        Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
      )
    }
    .map_err(|error| format!("The Windows preview GPU could not be opened: {error}"))?;
    let device = device.ok_or_else(|| "D3D11 returned no preview device".to_owned())?;
    let context = context.ok_or_else(|| "D3D11 returned no preview context".to_owned())?;
    let multithread: ID3D10Multithread = device.cast().map_err(|error| error.to_string())?;
    let _ = unsafe { multithread.SetMultithreadProtected(true) };
    let dxgi: IDXGIDevice = device.cast().map_err(|error| error.to_string())?;
    let adapter: IDXGIAdapter = unsafe { dxgi.GetAdapter() }.map_err(|error| error.to_string())?;
    let factory: IDXGIFactory2 =
      unsafe { adapter.GetParent() }.map_err(|error| error.to_string())?;
    let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }
      .map_err(|error| format!("DirectComposition could not use the preview GPU: {error}"))?;
    // The non-topmost target is the critical Windows equivalent of inserting
    // the Metal view immediately below WKWebView: WebView2 remains a child
    // window above this GPU visual tree, so its DOM OSCs paint last.
    let target = unsafe { composition.CreateTargetForHwnd(host, false) }
      .map_err(|error| format!("The Windows preview compositor could not attach: {error}"))?;
    let root = unsafe { composition.CreateVisual() }
      .map_err(|error| format!("The Windows preview visual tree could not be created: {error}"))?;
    unsafe { target.SetRoot(&root) }
      .map_err(|error| format!("The Windows preview visual tree could not be attached: {error}"))?;
    let backdrop = Backdrop::new(&composition, &factory, &device, &root)?;
    backdrop.paint(&context, [0.09, 0.09, 0.10, 1.0])?;
    unsafe { composition.Commit() }
      .map_err(|error| format!("The Windows preview compositor could not start: {error}"))?;
    let compositor = compositor::Compositor::new(&device)?;
    Ok(Self {
      backdrop,
      compositor,
      composition,
      context,
      device,
      factory,
      root,
      _target: target,
    })
  }

  fn pane(&self, below: Option<&IDCompositionVisual>) -> Result<Pane, String> {
    let description = DXGI_SWAP_CHAIN_DESC1 {
      Width: 2,
      Height: 2,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      Stereo: false.into(),
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
      BufferCount: 2,
      Scaling: DXGI_SCALING_STRETCH,
      SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
      AlphaMode: windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_PREMULTIPLIED,
      Flags: 0,
    };
    let swap_chain = unsafe {
      self
        .factory
        .CreateSwapChainForComposition(&self.device, &description, None)
    }
    .map_err(|error| format!("The Windows preview swap chain could not be created: {error}"))?;
    let swap_chain = swap_chain
      .cast::<IDXGISwapChain3>()
      .map_err(|error| format!("The Windows preview requires a flip-model swap chain: {error}"))?;
    let visual = unsafe { self.composition.CreateVisual() }
      .map_err(|error| format!("The Windows preview pane visual could not be created: {error}"))?;
    let scale_transform = unsafe { self.composition.CreateScaleTransform() }.map_err(|error| {
      format!("The Windows preview pane transform could not be created: {error}")
    })?;
    let clip = unsafe { self.composition.CreateRectangleClip() }
      .map_err(|error| format!("The Windows preview pane clip could not be created: {error}"))?;
    (|| -> windows::core::Result<()> {
      unsafe {
        visual.SetContent(&swap_chain)?;
        visual.SetTransform(&scale_transform)?;
        visual.SetClip(&clip)?;
        // Screenshot layer panes share one full-canvas rect, so sibling order
        // is the layer order: each pane sits directly above the nearest
        // lower-index pane, leaving higher indices frontmost as on macOS.
        self
          .root
          .AddVisual(&visual, true, Some(below.unwrap_or(&self.backdrop.visual)))?;
        self.composition.Commit()?;
      }
      Ok(())
    })()
    .map_err(|error| format!("The Windows preview pane could not be attached: {error}"))?;
    Ok(Pane {
      buffer_size: (2, 2),
      clip,
      clip_edges: (0, 0, 2, 2),
      content_size: (2, 2),
      display_size: (2, 2),
      last_composition: None,
      pending_geometry: false,
      pending_present: false,
      position: (0, 0),
      scale_transform,
      seen: true,
      source: None,
      source_token: None,
      swap_chain,
      visual,
    })
  }
}

impl Backdrop {
  fn new(
    composition: &IDCompositionDevice,
    factory: &IDXGIFactory2,
    device: &ID3D11Device,
    root: &IDCompositionVisual,
  ) -> Result<Self, String> {
    let description = DXGI_SWAP_CHAIN_DESC1 {
      Width: 2,
      Height: 2,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      Stereo: false.into(),
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
      BufferCount: 2,
      Scaling: DXGI_SCALING_STRETCH,
      SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
      AlphaMode: windows::Win32::Graphics::Dxgi::Common::DXGI_ALPHA_MODE_PREMULTIPLIED,
      Flags: 0,
    };
    let swap_chain = unsafe { factory.CreateSwapChainForComposition(device, &description, None) }
      .and_then(|chain| chain.cast::<IDXGISwapChain3>())
      .map_err(|error| format!("The Windows preview backstop could not be created: {error}"))?;
    let visual = unsafe { composition.CreateVisual() }.map_err(|error| {
      format!("The Windows preview backstop visual could not be created: {error}")
    })?;
    let scale_transform = unsafe { composition.CreateScaleTransform() }.map_err(|error| {
      format!("The Windows preview backstop transform could not be created: {error}")
    })?;
    (|| -> windows::core::Result<()> {
      unsafe {
        visual.SetContent(&swap_chain)?;
        visual.SetTransform(&scale_transform)?;
        visual.SetOffsetX2(-100_000.0)?;
        // This is the native equivalent of macOS's opaque container layer: it
        // sits below every video pane but fills the complete preview viewport.
        root.AddVisual(&visual, false, None::<&IDCompositionVisual>)?;
      }
      Ok(())
    })()
    .map_err(|error| format!("The Windows preview backstop could not be attached: {error}"))?;
    Ok(Self {
      scale_transform,
      swap_chain,
      visual,
    })
  }

  fn paint(&self, context: &ID3D11DeviceContext, colour: [f64; 4]) -> Result<(), String> {
    let index = unsafe { self.swap_chain.GetCurrentBackBufferIndex() };
    let target = unsafe { self.swap_chain.GetBuffer::<ID3D11Texture2D>(index) }
      .map_err(|error| format!("The Windows preview backstop has no buffer: {error}"))?;
    let resource: ID3D11Resource = target.cast().map_err(|error| error.to_string())?;
    let device = unsafe { target.GetDevice() }.map_err(|error| error.to_string())?;
    let mut view: Option<ID3D11RenderTargetView> = None;
    unsafe { device.CreateRenderTargetView(&resource, None, Some(&mut view)) }
      .map_err(|error| format!("The Windows preview backstop could not be painted: {error}"))?;
    let view = view.ok_or_else(|| "D3D11 created no preview backstop view".to_owned())?;
    let alpha = colour[3].clamp(0.0, 1.0) as f32;
    let colour = [
      colour[0].clamp(0.0, 1.0) as f32 * alpha,
      colour[1].clamp(0.0, 1.0) as f32 * alpha,
      colour[2].clamp(0.0, 1.0) as f32 * alpha,
      alpha,
    ];
    unsafe { context.ClearRenderTargetView(&view, &colour) };
    unsafe { self.swap_chain.Present(0, DXGI_PRESENT(0)) }
      .ok()
      .map_err(|error| format!("The Windows preview backstop could not be presented: {error}"))
  }

  fn set_geometry(&self, rect: PreviewSurfaceRect, scale: f64) {
    if rect.width < 1.0 || rect.height < 1.0 {
      self.hide();
      return;
    }
    let (x, right) = window::scaled_edges(rect.x, rect.width, scale);
    let (y, bottom) = window::scaled_edges(rect.y, rect.height, scale);
    let _ = unsafe {
      self
        .visual
        .SetOffsetX2(x as f32)
        .and_then(|_| self.visual.SetOffsetY2(y as f32))
        .and_then(|_| {
          self
            .scale_transform
            .SetScaleX2((right - x).max(2) as f32 / 2.0)
        })
        .and_then(|_| {
          self
            .scale_transform
            .SetScaleY2((bottom - y).max(2) as f32 / 2.0)
        })
    };
  }

  fn hide(&self) {
    let _ = unsafe { self.visual.SetOffsetX2(-100_000.0) };
  }
}

impl Pane {
  fn display_aspect_matches_buffer(&self) -> bool {
    let first = f64::from(self.content_size.0) * f64::from(self.display_size.1);
    let second = f64::from(self.content_size.1) * f64::from(self.display_size.0);
    let scale = first.max(second).max(1.0);
    (first - second).abs() / scale < 0.005
  }

  fn update_geometry(&self) -> windows::core::Result<()> {
    let content_width = self.content_size.0.max(1) as f32;
    let content_height = self.content_size.1.max(1) as f32;
    let display_width = self.display_size.0.max(1) as f32;
    let display_height = self.display_size.1.max(1) as f32;
    let (clip_left, clip_top, clip_right, clip_bottom) = self.clip_edges;
    unsafe {
      self.visual.SetOffsetX2(self.position.0 as f32)?;
      self.visual.SetOffsetY2(self.position.1 as f32)?;
      self
        .scale_transform
        .SetScaleX2(display_width / content_width)?;
      self
        .scale_transform
        .SetScaleY2(display_height / content_height)?;
      self
        .clip
        .SetLeft2(clip_left as f32 * content_width / display_width)?;
      self
        .clip
        .SetTop2(clip_top as f32 * content_height / display_height)?;
      self
        .clip
        .SetRight2(clip_right as f32 * content_width / display_width)?;
      self
        .clip
        .SetBottom2(clip_bottom as f32 * content_height / display_height)?;
    }
    Ok(())
  }

  fn hide(&self) {
    let _ = unsafe { self.visual.SetOffsetX2(-100_000.0) };
  }
}

impl RecordingPreviewSurface {
  fn present_cached_source(
    &self,
    pane: &mut Pane,
    settings: &ScreenshotOutputSettings,
    composition: ComposedFrame,
  ) -> Result<bool, String> {
    self.present_cached_source_with_camera(pane, settings, composition, None)
  }

  fn present_cached_source_with_camera(
    &self,
    pane: &mut Pane,
    settings: &ScreenshotOutputSettings,
    composition: ComposedFrame,
    camera: Option<(&compositor::SourceTexture, BakeGeometry, bool, bool)>,
  ) -> Result<bool, String> {
    crate::screenshots::output_dimensions(settings)?;
    // Keep one stable output chain for the current preview resolution. The
    // source texture is cached separately and edits only redraw this target.
    // Buffers are only reallocated when the output outgrows them (with
    // headroom, so an interactive resize reallocates rarely rather than per
    // pointer move - per-move ResizeBuffers churn stalls the compositor).
    // The visual's scale transform and clip in `update_geometry` map and
    // bound exactly the drawn `content_size` region, so the unused margin of
    // the larger buffer is never composed. (`SetSourceSize` cannot express
    // this here: with the mandatory stretch scaling of a composition swap
    // chain it rescales the region to the buffer bounds and warps.)
    let output_size = (settings.width, settings.height);
    let resized = pane.content_size != output_size;
    if pane.buffer_size.0 < output_size.0 || pane.buffer_size.1 < output_size.1 {
      let buffer = (
        output_size.0.max(pane.buffer_size.0).next_multiple_of(256),
        output_size.1.max(pane.buffer_size.1).next_multiple_of(256),
      );
      unsafe {
        pane.swap_chain.ResizeBuffers(
          2,
          buffer.0,
          buffer.1,
          DXGI_FORMAT_B8G8R8A8_UNORM,
          DXGI_SWAP_CHAIN_FLAG(0),
        )
      }
      .map_err(|error| format!("The Windows composed preview could not resize: {error}"))?;
      pane.buffer_size = buffer;
    }
    if resized {
      pane.content_size = output_size;
    }
    pane.last_composition = Some(composition);
    let source = pane
      .source
      .as_ref()
      .ok_or_else(|| "The preview source texture is unavailable".to_owned())?;
    let buffer_index = unsafe { pane.swap_chain.GetCurrentBackBufferIndex() };
    let target = unsafe { pane.swap_chain.GetBuffer::<ID3D11Texture2D>(buffer_index) }
      .map_err(|error| format!("The composed preview has no back buffer: {error}"))?;
    // A foreground layer blends over the existing target, and a flip-discard
    // back buffer is undefined after each present: its uncovered pixels must
    // read as transparent, not as stale frame data.
    if composition.foreground_only {
      let resource: ID3D11Resource = target.cast().map_err(|error| error.to_string())?;
      let mut view: Option<ID3D11RenderTargetView> = None;
      unsafe {
        self
          .inner
          .gpu
          .device
          .CreateRenderTargetView(&resource, None, Some(&mut view))
      }
      .map_err(|error| format!("The layer preview could not clear its target: {error}"))?;
      if let Some(view) = view {
        unsafe { self.inner.gpu.context.ClearRenderTargetView(&view, &[0.0; 4]) };
      }
    }
    self.inner.gpu.compositor.draw_with_camera(
      &self.inner.gpu.context,
      &target,
      source,
      settings,
      composition,
      camera,
    )?;
    unsafe { self.inner.gpu.context.Flush() };
    // Inside an open batch the frame is parked: the closing guard presents
    // every pane and commits every pending geometry in one flush, so sibling
    // layers change on the same compositor pass.
    if self.inner.batch_depth.load(Ordering::Acquire) > 0 {
      pane.pending_present = true;
      if resized {
        pane.pending_geometry = true;
      }
      return Ok(true);
    }
    unsafe { pane.swap_chain.Present(0, DXGI_PRESENT(0)) }
      .ok()
      .map_err(|error| format!("The composed preview could not present: {error}"))?;
    // Publish resized or deferred geometry only after the replacement frame
    // exists, immediately behind its present so both land in one pass.
    if resized || pane.pending_geometry {
      pane.pending_geometry = false;
      pane.update_geometry().map_err(|error| error.to_string())?;
      unsafe { self.inner.gpu.composition.Commit() }.map_err(|error| error.to_string())?;
    }
    Ok(true)
  }

  pub(crate) fn existing() -> Result<std::sync::Arc<Self>, String> {
    let inner = PREVIEW_SURFACE
      .get()
      .ok_or_else(|| "The Windows GPU compositor has not been opened".to_owned())?
      .as_ref()
      .map_err(Clone::clone)?;
    Ok(std::sync::Arc::new(Self {
      inner: std::sync::Arc::clone(inner),
    }))
  }

  pub(crate) fn from_window(window: &WebviewWindow) -> Result<Self, String> {
    let inner = PREVIEW_SURFACE
      .get_or_init(|| {
        let host = window.hwnd().map_err(|error| error.to_string())?;
        let host = HWND(host.0);
        Ok(std::sync::Arc::new(SurfaceInner {
          batch_depth: AtomicU32::new(0),
          gpu: Gpu::new(host)?,
          state: Mutex::new(SurfaceState {
            backdrop: [0.09, 0.09, 0.10, 1.0],
            camera_source: None,
            panes: Vec::new(),
            primary_composition: None,
            scale: 1.0,
            viewport: PreviewSurfaceRect {
              height: 0.0,
              width: 0.0,
              x: 0.0,
              y: 0.0,
            },
          }),
        }))
      })
      .as_ref()
      .map_err(Clone::clone)?;
    Ok(Self {
      inner: std::sync::Arc::clone(inner),
    })
  }

  pub(crate) fn device(&self) -> ID3D11Device {
    self.inner.gpu.device.clone()
  }

  pub(crate) fn export_compositor(
    &self,
    source_size: (u32, u32),
    output_size: (u32, u32),
  ) -> Result<WindowsExportCompositor, String> {
    let source = self
      .inner
      .gpu
      .compositor
      .source(&self.inner.gpu.device, source_size)?;
    Ok(WindowsExportCompositor {
      camera: None,
      inner: std::sync::Arc::clone(&self.inner),
      output_size,
      source,
    })
  }

  pub(crate) fn export_compositor_with_camera(
    &self,
    source_size: (u32, u32),
    camera_size: (u32, u32),
    output_size: (u32, u32),
  ) -> Result<WindowsExportCompositor, String> {
    let mut compositor = self.export_compositor(source_size, output_size)?;
    compositor.camera = Some(
      self
        .inner
        .gpu
        .compositor
        .source(&self.inner.gpu.device, camera_size)?,
    );
    Ok(compositor)
  }

  pub(crate) fn set_scale(&self, scale: f64) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.scale = scale.max(0.1);
    }
  }

  pub(crate) fn set_viewport(&self, rect: PreviewSurfaceRect, backdrop: [f64; 4]) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.viewport = rect;
      self.inner.gpu.backdrop.set_geometry(rect, state.scale);
      if state.backdrop != backdrop
        && self
          .inner
          .gpu
          .backdrop
          .paint(&self.inner.gpu.context, backdrop)
          .is_ok()
      {
        state.backdrop = backdrop;
      }
    }
  }

  pub(crate) fn begin_layout(&self) {
    if let Ok(mut state) = self.inner.state.lock() {
      for pane in state.panes.iter_mut().flatten() {
        pane.seen = false;
      }
    }
  }

  /// `defer_resize` holds the new pane geometry back until the re-composed
  /// frame for it presents, so rect and pixels reach the compositor together
  /// instead of the old buffer shifting into the new rect for a tick.
  pub(crate) fn layout(&self, index: u32, rect: PreviewSurfaceRect, defer_resize: bool) {
    let Ok(mut state) = self.inner.state.lock() else {
      return;
    };
    let index = index as usize;
    if state.panes.len() <= index {
      state.panes.resize_with(index + 1, || None);
    }
    if state.panes[index].is_none() {
      let below = state.panes[..index]
        .iter()
        .rev()
        .flatten()
        .next()
        .map(|pane| pane.visual.clone());
      state.panes[index] = self.inner.gpu.pane(below.as_ref()).ok();
    }
    let scale = state.scale;
    let viewport = state.viewport;
    let Some(pane) = state.panes[index].as_mut() else {
      return;
    };
    pane.seen = true;
    let (x, right) = window::scaled_edges(viewport.x + rect.x, rect.width, scale);
    let (y, bottom) = window::scaled_edges(viewport.y + rect.y, rect.height, scale);
    let width = (right - x).max(2);
    let height = (bottom - y).max(2);
    let (viewport_x, viewport_right) = window::scaled_edges(viewport.x, viewport.width, scale);
    let (viewport_y, viewport_bottom) = window::scaled_edges(viewport.y, viewport.height, scale);
    pane.position = (x, y);
    pane.display_size = (width, height);
    pane.clip_edges = (
      (viewport_x - x).clamp(0, width),
      (viewport_y - y).clamp(0, height),
      (viewport_right - x).clamp(0, width),
      (viewport_bottom - y).clamp(0, height),
    );
    // With a present on the way the geometry waits for it, so the pane's rect
    // and its freshly composed pixels land in the same commit. A pure pan (no
    // present coming) applies at once - but never while the DOM rect's aspect
    // has outrun the buffer: stretching the old frame into a new-aspect rect
    // for one transaction is exactly the jitter this avoids. Deferred
    // geometry is published by the pane's next present or the batch flush.
    if defer_resize {
      pane.pending_geometry = true;
    } else if pane.display_aspect_matches_buffer() {
      pane.pending_geometry = false;
      let _ = pane.update_geometry();
    } else {
      pane.pending_geometry = true;
    }
  }

  /// Opens a present batch: frames drawn until the guard drops are parked on
  /// their panes and published by one flush together with every geometry
  /// deferred by `layout`. Dropping the guard flushes even when nothing was
  /// presented, so a deferred layout never strands the panes.
  pub(crate) fn present_batch(&self) -> PresentBatch<'_> {
    self.inner.batch_depth.fetch_add(1, Ordering::AcqRel);
    PresentBatch { surface: self }
  }

  pub(crate) fn finish_layout(&self) {
    if let Ok(mut state) = self.inner.state.lock() {
      for pane in state
        .panes
        .iter_mut()
        .flatten()
        .filter(|pane| !pane.seen)
      {
        pane.hide();
        // A hidden pane has nothing stale to show; drop leftovers so a later
        // flush cannot resurrect it at a parked offset.
        pane.pending_geometry = false;
        pane.pending_present = false;
      }
      // An open batch commits for everything on its flush; a second
      // commit-and-wait here would add a display tick of latency to every
      // layout that presents in the same invoke.
      if self.inner.batch_depth.load(Ordering::Acquire) > 0 {
        return;
      }
      if unsafe { self.inner.gpu.composition.Commit() }.is_ok() {
        // Commit is otherwise only queued. Waiting here prevents rapid DOM
        // pans from building a DirectComposition transaction backlog in which
        // the OSCs visibly outrun the video pane. The frontend already keeps
        // only the newest layout while this one reaches the compositor.
        let _ = unsafe { self.inner.gpu.composition.WaitForCommitCompletion() };
      }
    }
  }

  pub(crate) fn present_composed_texture(
    &self,
    index: u32,
    texture: &ID3D11Texture2D,
    subresource: u32,
    size: (u32, u32),
    settings: &ScreenshotOutputSettings,
    composition: ComposedFrame,
  ) -> Result<bool, String> {
    let Ok(mut state) = self.inner.state.lock() else {
      return Ok(false);
    };
    let Some(pane) = state.panes.get_mut(index as usize).and_then(Option::as_mut) else {
      return Ok(false);
    };
    if pane
      .source
      .as_ref()
      .is_none_or(|source| source.size != size)
    {
      pane.source = Some(
        self
          .inner
          .gpu
          .compositor
          .source(&self.inner.gpu.device, size)?,
      );
      pane.source_token = None;
    }
    let source = pane
      .source
      .as_ref()
      .ok_or_else(|| "The preview source texture is unavailable".to_owned())?;
    compositor::Compositor::copy_source(&self.inner.gpu.context, source, texture, subresource)?;
    pane.source_token = None;
    self.present_cached_source(pane, settings, composition)
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_baked_camera_texture(
    &self,
    index: u32,
    texture: &ID3D11Texture2D,
    subresource: u32,
    size: (u32, u32),
    settings: &ScreenshotOutputSettings,
    overlay: crate::exports::CameraOverlaySettings,
    drop_shadow: bool,
    camera_on_top: bool,
    composition: ComposedFrame,
  ) -> Result<bool, String> {
    let Ok(mut state) = self.inner.state.lock() else {
      return Ok(false);
    };
    if index == 1 {
      if state
        .camera_source
        .as_ref()
        .is_none_or(|source| source.size != size)
      {
        state.camera_source = Some(
          self
            .inner
            .gpu
            .compositor
            .source(&self.inner.gpu.device, size)?,
        );
      }
      if let Some(camera) = &state.camera_source {
        compositor::Compositor::copy_source(&self.inner.gpu.context, camera, texture, subresource)?;
      }
    } else {
      state.primary_composition = Some(composition);
      let Some(pane) = state.panes.first_mut().and_then(Option::as_mut) else {
        return Ok(false);
      };
      if pane
        .source
        .as_ref()
        .is_none_or(|source| source.size != size)
      {
        pane.source = Some(
          self
            .inner
            .gpu
            .compositor
            .source(&self.inner.gpu.device, size)?,
        );
        pane.source_token = None;
      }
      let source = pane
        .source
        .as_ref()
        .ok_or_else(|| "The preview source texture is unavailable".to_owned())?;
      compositor::Compositor::copy_source(&self.inner.gpu.context, source, texture, subresource)?;
      pane.source_token = None;
    }
    let Some(camera) = state.camera_source.clone() else {
      let Some(pane) = state.panes.first_mut().and_then(Option::as_mut) else {
        return Ok(true);
      };
      if pane.source.is_none() {
        return Ok(true);
      }
      return self.present_cached_source(pane, settings, composition);
    };
    let composition = if index == 1 {
      state.primary_composition.unwrap_or(composition)
    } else {
      composition
    };
    let geometry = crate::exports::media_preview::bake_geometry(BakedVideoExportOptions {
      camera_drop_shadow: drop_shadow,
      camera_height: camera.size.1,
      camera_width: camera.size.0,
      overlay,
      screen_height: settings.height,
      screen_width: settings.width,
      video: VideoExportOptions {
        compression: 0,
        resolution_scale_percent: 100,
        source_scale_percent: 100,
      },
    })?;
    let Some(pane) = state.panes.first_mut().and_then(Option::as_mut) else {
      return Ok(true);
    };
    if pane.source.is_none() {
      return Ok(true);
    }
    self.present_cached_source_with_camera(
      pane,
      settings,
      composition,
      Some((&camera, geometry, drop_shadow, camera_on_top)),
    )
  }

  /// Redraws the paused stills from their cached full-resolution sources and
  /// compositions with the given output settings - no decoder round trip, so
  /// a canvas resize follows the pointer instead of trailing a Media
  /// Foundation reopen. Returns `Ok(false)` when a needed frame is not
  /// cached yet and the decoder has to supply it. `bake_camera` means "bake
  /// with an available camera track" - the caller folds the track's
  /// existence in, exactly like the present path does.
  #[allow(clippy::too_many_arguments)]
  pub(crate) fn redraw_still(
    &self,
    bake_camera: bool,
    primary: &ScreenshotOutputSettings,
    camera_settings: &ScreenshotOutputSettings,
    overlay: crate::exports::CameraOverlaySettings,
    drop_shadow: bool,
    camera_on_top: bool,
  ) -> Result<bool, String> {
    let batch = self.present_batch();
    {
      let Ok(mut state) = self.inner.state.lock() else {
        return Ok(false);
      };
      let camera_source = state.camera_source.clone();
      let Some(pane) = state.panes.first_mut().and_then(Option::as_mut) else {
        return Ok(false);
      };
      let (Some(composition), Some(_)) = (pane.last_composition, pane.source.as_ref()) else {
        return Ok(false);
      };
      if bake_camera {
        // The camera texture is only cached while baked presents run; right
        // after a bake toggle it is absent (or stale) and the decoder must
        // deliver it. Never draw a baked still without its camera.
        let Some(camera) = camera_source.as_ref() else {
          return Ok(false);
        };
        {
          let geometry = crate::exports::media_preview::bake_geometry(BakedVideoExportOptions {
            camera_drop_shadow: drop_shadow,
            camera_height: camera.size.1,
            camera_width: camera.size.0,
            overlay,
            screen_height: primary.height,
            screen_width: primary.width,
            video: VideoExportOptions {
              compression: 0,
              resolution_scale_percent: 100,
              source_scale_percent: 100,
            },
          })?;
          self.present_cached_source_with_camera(
            pane,
            primary,
            composition,
            Some((camera, geometry, drop_shadow, camera_on_top)),
          )?;
        }
      } else {
        self.present_cached_source(pane, primary, composition)?;
      }
      if !bake_camera {
        if let Some(pane) = state.panes.get_mut(1).and_then(Option::as_mut) {
          if let (Some(composition), Some(_)) = (pane.last_composition, pane.source.as_ref()) {
            self.present_cached_source(pane, camera_settings, composition)?;
          }
        }
      }
    }
    drop(batch);
    Ok(true)
  }

  /// Renders one explicit clipboard frame through the exact preview shader,
  /// then performs the single unavoidable GPU readback required by the
  /// Windows clipboard. Live preview never calls this path.
  pub(in crate::exports) fn compose_screenshot_layers_to_image(
    &self,
    layers: &[(&CapturedImage, ScreenshotOutputSettings)],
  ) -> Result<CapturedImage, String> {
    let (_, first_settings) = layers
      .first()
      .ok_or_else(|| "The screenshot workspace is empty".to_owned())?;
    let _state = self
      .inner
      .state
      .lock()
      .map_err(|_| "The Windows preview surface is unavailable".to_owned())?;
    let output_size = crate::screenshots::output_dimensions(first_settings)?;
    let target_description = D3D11_TEXTURE2D_DESC {
      Width: output_size.0,
      Height: output_size.1,
      MipLevels: 1,
      ArraySize: 1,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
      ..Default::default()
    };
    let mut target = None;
    unsafe {
      self
        .inner
        .gpu
        .device
        .CreateTexture2D(&target_description, None, Some(&mut target))
    }
    .map_err(|error| format!("The screenshot render target could not be created: {error}"))?;
    let target = target.ok_or_else(|| "D3D11 created no screenshot render target".to_owned())?;

    for (index, (image, settings)) in layers.iter().enumerate() {
      if crate::screenshots::output_dimensions(settings)? != output_size {
        return Err("The screenshot layers do not share a canvas size".to_owned());
      }
      let source = self
        .inner
        .gpu
        .compositor
        .screenshot_source(&self.inner.gpu.device, image)?;
      self.inner.gpu.compositor.draw_with_camera(
        &self.inner.gpu.context,
        &target,
        &source,
        settings,
        ComposedFrame {
          cursor: None,
          foreground_only: index > 0,
          seconds: 0.0,
        },
        None,
      )?;
    }
    unsafe { self.inner.gpu.context.Flush() };
    self.readback_bgra(&target, target_description, output_size, "screenshot")
  }

  pub(in crate::exports) fn compose_texture_to_image(
    &self,
    texture: &ID3D11Texture2D,
    subresource: u32,
    source_size: (u32, u32),
    settings: &ScreenshotOutputSettings,
    composition: ComposedFrame,
    camera: Option<ClipboardCamera<'_>>,
  ) -> Result<CapturedImage, String> {
    let _state = self
      .inner
      .state
      .lock()
      .map_err(|_| "The Windows preview surface is unavailable".to_owned())?;
    let output_size = crate::screenshots::output_dimensions(settings)?;
    let source = self
      .inner
      .gpu
      .compositor
      .source(&self.inner.gpu.device, source_size)?;
    compositor::Compositor::copy_source(&self.inner.gpu.context, &source, texture, subresource)?;
    let camera_source = camera
      .map(|(_, _, size, _, _, _)| {
        self
          .inner
          .gpu
          .compositor
          .source(&self.inner.gpu.device, size)
      })
      .transpose()?;
    if let (Some(camera_source), Some((texture, subresource, _, _, _, _))) =
      (&camera_source, camera)
    {
      compositor::Compositor::copy_source(
        &self.inner.gpu.context,
        camera_source,
        texture,
        subresource,
      )?;
    }
    let target_description = D3D11_TEXTURE2D_DESC {
      Width: output_size.0,
      Height: output_size.1,
      MipLevels: 1,
      ArraySize: 1,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
      ..Default::default()
    };
    let mut target = None;
    unsafe {
      self
        .inner
        .gpu
        .device
        .CreateTexture2D(&target_description, None, Some(&mut target))
    }
    .map_err(|error| format!("The clipboard render target could not be created: {error}"))?;
    let target = target.ok_or_else(|| "D3D11 created no clipboard render target".to_owned())?;
    self.inner.gpu.compositor.draw_with_camera(
      &self.inner.gpu.context,
      &target,
      &source,
      settings,
      composition,
      camera.and_then(|(_, _, _, geometry, drop_shadow, camera_on_top)| {
        camera_source
          .as_ref()
          .map(|source| (source, geometry, drop_shadow, camera_on_top))
      }),
    )?;

    self.readback_bgra(&target, target_description, output_size, "clipboard")
  }

  fn readback_bgra(
    &self,
    target: &ID3D11Texture2D,
    target_description: D3D11_TEXTURE2D_DESC,
    output_size: (u32, u32),
    purpose: &str,
  ) -> Result<CapturedImage, String> {
    let staging_description = D3D11_TEXTURE2D_DESC {
      Usage: D3D11_USAGE_STAGING,
      BindFlags: 0,
      CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
      ..target_description
    };
    let mut staging = None;
    unsafe {
      self
        .inner
        .gpu
        .device
        .CreateTexture2D(&staging_description, None, Some(&mut staging))
    }
    .map_err(|error| format!("The {purpose} readback texture could not be created: {error}"))?;
    let staging = staging.ok_or_else(|| format!("D3D11 created no {purpose} readback texture"))?;
    let target_resource: windows::Win32::Graphics::Direct3D11::ID3D11Resource =
      target.cast().map_err(|error| error.to_string())?;
    let staging_resource: windows::Win32::Graphics::Direct3D11::ID3D11Resource =
      staging.cast().map_err(|error| error.to_string())?;
    unsafe {
      self
        .inner
        .gpu
        .context
        .CopyResource(&staging_resource, &target_resource);
    }
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
      self
        .inner
        .gpu
        .context
        .Map(&staging_resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
    }
    .map_err(|error| format!("The {purpose} frame could not be read back: {error}"))?;
    let row_bytes = output_size.0 as usize * 4;
    let mut rgba = vec![0_u8; row_bytes * output_size.1 as usize];
    if mapped.pData.is_null() || mapped.RowPitch < row_bytes as u32 {
      unsafe { self.inner.gpu.context.Unmap(&staging_resource, 0) };
      return Err(format!("D3D11 returned invalid {purpose} pixels"));
    }
    for row in 0..output_size.1 as usize {
      let source_row = unsafe {
        std::slice::from_raw_parts(
          mapped
            .pData
            .cast::<u8>()
            .add(row * mapped.RowPitch as usize),
          row_bytes,
        )
      };
      let target_row = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
      for (source_pixel, target_pixel) in source_row
        .chunks_exact(4)
        .zip(target_row.chunks_exact_mut(4))
      {
        target_pixel.copy_from_slice(&[
          source_pixel[2],
          source_pixel[1],
          source_pixel[0],
          source_pixel[3],
        ]);
      }
    }
    unsafe { self.inner.gpu.context.Unmap(&staging_resource, 0) };
    Ok(CapturedImage {
      height: output_size.1,
      rgba,
      width: output_size.0,
    })
  }

  #[allow(dead_code)]
  pub(crate) fn present(&self, _index: u32, _image: &CapturedImage) -> bool {
    false
  }

  #[allow(clippy::too_many_arguments)]
  pub(crate) fn present_composed(
    &self,
    index: u32,
    source_token: u64,
    source: &CapturedImage,
    settings: &ScreenshotOutputSettings,
    seconds: f64,
    _cursor: Option<&CapturedImage>,
    _camera: Option<&CapturedImage>,
    _overlay: Option<&StillOverlay>,
    _clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    let Ok(mut state) = self.inner.state.lock() else {
      return Ok(false);
    };
    let Some(pane) = state.panes.get_mut(index as usize).and_then(Option::as_mut) else {
      return Ok(false);
    };
    let source_size = (source.width, source.height);
    if pane.source_token != Some(source_token)
      || pane
        .source
        .as_ref()
        .is_none_or(|texture| texture.size != source_size)
    {
      let texture = self
        .inner
        .gpu
        .compositor
        .screenshot_source(&self.inner.gpu.device, source)?;
      pane.source = Some(texture);
      pane.source_token = Some(source_token);
    }
    self.present_cached_source(
      pane,
      settings,
      ComposedFrame {
        cursor: None,
        foreground_only: false,
        seconds,
      },
    )
  }

  pub(crate) fn present_screenshot_layer(
    &self,
    index: u32,
    source_token: u64,
    source: &CapturedImage,
    settings: &ScreenshotOutputSettings,
    foreground_only: bool,
  ) -> Result<bool, String> {
    let Ok(mut state) = self.inner.state.lock() else {
      return Ok(false);
    };
    let Some(pane) = state.panes.get_mut(index as usize).and_then(Option::as_mut) else {
      return Ok(false);
    };
    let source_size = (source.width, source.height);
    if pane.source_token != Some(source_token)
      || pane
        .source
        .as_ref()
        .is_none_or(|texture| texture.size != source_size)
    {
      let texture = self
        .inner
        .gpu
        .compositor
        .screenshot_source(&self.inner.gpu.device, source)?;
      pane.source = Some(texture);
      pane.source_token = Some(source_token);
    }
    self.present_cached_source(
      pane,
      settings,
      ComposedFrame {
        cursor: None,
        foreground_only,
        seconds: 0.0,
      },
    )
  }

  #[allow(clippy::too_many_arguments)]
  #[allow(dead_code)]
  pub(crate) fn present_composed_pixels(
    &self,
    _index: u32,
    _source_token: u64,
    _source_pixels: *mut std::ffi::c_void,
    _source_size: (u32, u32),
    _settings: &ScreenshotOutputSettings,
    _seconds: f64,
    _cursor: Option<&CapturedImage>,
    _camera: Option<&CapturedImage>,
    _camera_pixels: Option<*mut std::ffi::c_void>,
    _overlay: Option<&StillOverlay>,
    _clip_cursor_at_video_edge: bool,
  ) -> Result<bool, String> {
    Ok(false)
  }

  pub(crate) fn hide(&self) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.camera_source = None;
      state.primary_composition = None;
      self.inner.gpu.backdrop.hide();
      for pane in state.panes.iter().flatten() {
        pane.hide();
      }
      let _ = unsafe { self.inner.gpu.composition.Commit() };
    }
  }
}

impl WindowsExportCompositor {
  pub(in crate::exports) fn compose_with_camera(
    &self,
    texture: &ID3D11Texture2D,
    subresource: u32,
    settings: &ScreenshotOutputSettings,
    composition: ComposedFrame,
    camera: Option<(&ID3D11Texture2D, u32, BakeGeometry, bool, bool)>,
  ) -> Result<ID3D11Texture2D, String> {
    let _state = self
      .inner
      .state
      .lock()
      .map_err(|_| "The Windows GPU compositor is unavailable".to_owned())?;
    compositor::Compositor::copy_source(
      &self.inner.gpu.context,
      &self.source,
      texture,
      subresource,
    )?;
    if let (Some(camera_source), Some((camera_texture, camera_subresource, _, _, _))) =
      (&self.camera, camera)
    {
      compositor::Compositor::copy_source(
        &self.inner.gpu.context,
        camera_source,
        camera_texture,
        camera_subresource,
      )?;
    }
    // Sink Writer retains DXGI surfaces and feeds the hardware encoder
    // asynchronously. A single repainted render target therefore lets a later
    // frame overwrite an earlier sample before Media Foundation consumes it.
    // Give each submitted sample its own texture; MF's sample owns that texture
    // until encoding completes and naturally bounds outstanding allocations
    // through Sink Writer backpressure.
    let description = D3D11_TEXTURE2D_DESC {
      Width: self.output_size.0,
      Height: self.output_size.1,
      MipLevels: 1,
      ArraySize: 1,
      Format: DXGI_FORMAT_B8G8R8A8_UNORM,
      SampleDesc: DXGI_SAMPLE_DESC {
        Count: 1,
        Quality: 0,
      },
      Usage: D3D11_USAGE_DEFAULT,
      BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
      ..Default::default()
    };
    let mut target = None;
    unsafe {
      self
        .inner
        .gpu
        .device
        .CreateTexture2D(&description, None, Some(&mut target))
    }
    .map_err(|error| format!("The Windows export target could not be created: {error}"))?;
    let target = target.ok_or_else(|| "D3D11 created no Windows export target".to_owned())?;
    self.inner.gpu.compositor.draw_with_camera(
      &self.inner.gpu.context,
      &target,
      &self.source,
      settings,
      composition,
      camera.and_then(|(_, _, geometry, drop_shadow, camera_on_top)| {
        self
          .camera
          .as_ref()
          .map(|source| (source, geometry, drop_shadow, camera_on_top))
      }),
    )?;
    unsafe { self.inner.gpu.context.Flush() };
    Ok(target)
  }
}

/// An open present batch. Dropping it presents every parked frame
/// back-to-back, applies every pending pane geometry, and commits once, so
/// all panes reach the compositor in (almost always) the same pass - the
/// DirectComposition counterpart of the macOS single-`CATransaction` batch.
pub(crate) struct PresentBatch<'a> {
  surface: &'a RecordingPreviewSurface,
}

impl Drop for PresentBatch<'_> {
  fn drop(&mut self) {
    let inner = &self.surface.inner;
    if inner.batch_depth.fetch_sub(1, Ordering::AcqRel) != 1 {
      return;
    }
    let Ok(mut state) = inner.state.lock() else {
      return;
    };
    for pane in state.panes.iter_mut().flatten() {
      if pane.pending_present {
        pane.pending_present = false;
        let _ = unsafe { pane.swap_chain.Present(0, DXGI_PRESENT(0)) }.ok();
      }
    }
    for pane in state.panes.iter_mut().flatten() {
      if pane.pending_geometry {
        pane.pending_geometry = false;
        let _ = pane.update_geometry();
      }
    }
    // Unconditional: `finish_layout` leaves its hides to this commit whenever
    // the batch was already open.
    if unsafe { inner.gpu.composition.Commit() }.is_ok() {
      // As in `finish_layout`: an unawaited commit backlog lets rapid drags
      // visibly desynchronise the panes from the DOM controls above them.
      let _ = unsafe { inner.gpu.composition.WaitForCommitCompletion() };
    }
  }
}
