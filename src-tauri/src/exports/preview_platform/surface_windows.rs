// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows preview surface: GPU frames presented by DirectComposition beneath
//! WebView2's child window. Media Foundation and this surface share one D3D11 device, so live
//! recording frames never enter system memory or cross Tauri IPC, while transparent webview
//! regions leave DOM controls above the video.

use std::{
  collections::HashMap,
  sync::{
    atomic::{AtomicU32, Ordering},
    Mutex, OnceLock,
  },
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
        DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR,
      },
      Dxgi::{
        Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
        IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGISwapChain3, DXGI_PRESENT,
        DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
        DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
      },
    },
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::GetWindowThreadProcessId,
  },
};

#[path = "surface_windows/compositor.rs"]
mod compositor;
#[path = "surface_windows/editor.rs"]
mod editor;
#[path = "surface_windows/selection.rs"]
mod selection;
#[path = "surface_windows/snapping.rs"]
mod snapping;
#[path = "surface_windows/window.rs"]
mod window;

use super::{
  workspace_editor::{
    apply_crop_move, apply_crop_resize, hit_test_display, rebase_display_fit, DisplayRect,
    DisplayTarget, NormalizedRect,
  },
  workspace_transform::WorkspaceTransform,
  PreviewCapabilities, PreviewSelection, PreviewSurfaceRect, SelectionCallback,
  SelectionGestureCallback, SelectionGestureOperation, SelectionGesturePhase, TransformCallback,
};
use crate::exports::media_preview::{BakeGeometry, BakedVideoExportOptions, VideoExportOptions};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

pub(super) const CAPABILITIES: PreviewCapabilities = PreviewCapabilities {
  native_workspace_editor: true,
  native_recording_preview: true,
  native_screenshot_preview: true,
};

pub(crate) struct StillOverlay;

const FRAME_LAYER_ID: u32 = u32::MAX;
const CENTERED_RESIZE_EDGE: u32 = 1 << 16;

/// Logical placement of a recording layer in the retained workspace. Windows
/// keeps the placement in the DirectComposition pane geometry rather than
/// baking it into an intermediate bitmap, but the type mirrors the Metal
/// backend so the recording pipeline can submit one platform-independent
/// workspace description.
#[repr(C)]
#[derive(Clone, Copy, Default)]
// Retained-workspace contract kept in parity with macOS; not wired on Windows yet.
#[allow(dead_code)]
pub(crate) struct NativeWorkspacePlacement {
  pub(crate) x: i32,
  pub(crate) y: i32,
  pub(crate) width: u32,
  pub(crate) height: u32,
}

/// One decoded source in the retained recording workspace. Native Windows
/// playback currently supplies RGBA frames; the pixel-buffer fields are kept
/// in the contract for parity with the zero-copy macOS path and are rejected
/// clearly until the Media Foundation texture path is wired here.
// Retained-workspace contract kept in parity with macOS; not wired on Windows yet.
#[allow(dead_code)]
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
  selection: Mutex<selection::SelectionOverlay>,
  _editor_target: IDCompositionTarget,
  _target: IDCompositionTarget,
}

struct Backdrop {
  scale_transform: IDCompositionScaleTransform,
  swap_chain: IDXGISwapChain3,
  visual: IDCompositionVisual,
}

struct Pane {
  /// Stable viewport-local geometry before the shared workspace transform.
  base_rect: PreviewSurfaceRect,
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
  /// The camera overlay the most recent present composed over the source, so a
  /// local redraw (magnifier, geometry) never drops the baked camera for a frame.
  last_camera: Option<(BakeGeometry, bool, bool)>,
  settings: Option<ScreenshotOutputSettings>,
  magnifier: Option<CropMagnifier>,
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

#[derive(Clone, Copy)]
struct CropMagnifier {
  display_box: [f32; 4],
  geometry: [f32; 4],
  options: [f32; 4],
}

struct SurfaceState {
  backdrop: [f64; 4],
  camera_source: Option<compositor::SourceTexture>,
  editor_active: bool,
  /// The immutable workspace state a live Frame resize re-flows from, and the
  /// marker that the native side - not the DOM - owns the pane geometry.
  frame_resize: Option<FrameResizeStart>,
  /// A Frame resize has ended and its committed layout has not arrived yet:
  /// that layout keeps the rebased transform instead of restoring one.
  frame_resize_committed: bool,
  gesture: Option<ActiveGesture>,
  last_pointer: (f64, f64),
  panes: Vec<Option<Pane>>,
  primary_composition: Option<ComposedFrame>,
  scale: f64,
  selection: Option<PreviewSelection>,
  selection_visible: bool,
  selection_snapping_enabled: bool,
  selection_snap_guide_x: Option<snapping::SnapGuide>,
  selection_snap_guide_y: Option<snapping::SnapGuide>,
  selection_targets: Vec<PreviewSelection>,
  viewport: PreviewSurfaceRect,
  workspace_natural_size: Option<(u32, u32)>,
  workspace_transform: WorkspaceTransform,
  workspace_transforms: HashMap<(u32, u32), WorkspaceTransform>,
}

#[derive(Clone, Copy)]
struct EditorGesture {
  edges: u32,
  last_delta: (f64, f64),
  last_scale: f64,
  operation: SelectionGestureOperation,
  pane_start: PreviewSurfaceRect,
  pointer_start: (f64, f64),
  selection_start: PreviewSelection,
}

/// Everything a Frame resize needs to stay reversible and drift-free: the
/// pane rectangles, the workspace transform and the canvas size as they were
/// when the drag began. Every pointer move re-derives the whole workspace
/// from these, exactly as the Metal backend re-derives it from
/// `selectionFramePaneStarts` / `selectionFrameZoomStart`, so a rebased zoom
/// can never feed back into the next move's geometry.
struct FrameResizeStart {
  natural_size: Option<(u32, u32)>,
  pane_rects: Vec<(usize, PreviewSurfaceRect)>,
  transform: WorkspaceTransform,
}

#[derive(Clone, Copy)]
enum ActiveGesture {
  Pan {
    pointer_start: (f64, f64),
    transform_start: WorkspaceTransform,
  },
  Selection(EditorGesture),
}

#[derive(Default)]
struct EditorCallbacks {
  gesture: Option<SelectionGestureCallback>,
  selection: Option<SelectionCallback>,
  transform: Option<TransformCallback>,
}

struct SurfaceInner {
  /// Open `present_batch` guards. While positive, presents park their frames
  /// on the pane and the closing guard publishes every frame and every
  /// pending geometry in one flush (the DirectComposition analogue of the
  /// macOS single-`CATransaction` batch).
  batch_depth: AtomicU32,
  callbacks: Mutex<EditorCallbacks>,
  editor: editor::EditorWindow,
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
  fn new(host: HWND, editor: HWND) -> Result<Self, String> {
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
    // The selection OSC lives in its own composition target on the editor
    // child window, which is kept above the WebView2 sibling. That window is
    // `WS_EX_NOREDIRECTIONBITMAP`, so DirectComposition owns all of its
    // content and the target is created topmost.
    let editor_target = unsafe { composition.CreateTargetForHwnd(editor, true) }
      .map_err(|error| format!("The Windows selection compositor could not attach: {error}"))?;
    let editor_root = unsafe { composition.CreateVisual() }
      .map_err(|error| format!("The Windows selection visual could not be created: {error}"))?;
    unsafe { editor_target.SetRoot(&editor_root) }
      .map_err(|error| format!("The Windows selection visual could not be attached: {error}"))?;
    let compositor = compositor::Compositor::new(&device)?;
    let selection =
      selection::SelectionOverlay::new(&device, &factory, &composition, &editor_root)?;
    unsafe { composition.Commit() }
      .map_err(|error| format!("The Windows preview compositor could not start: {error}"))?;
    Ok(Self {
      backdrop,
      compositor,
      composition,
      context,
      device,
      factory,
      root,
      selection: Mutex::new(selection),
      _editor_target: editor_target,
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
        // DirectComposition defaults to nearest-neighbour bitmap sampling.
        // Whatever residual scale the pane transform carries must resample
        // the frame rather than decimate it.
        visual.SetBitmapInterpolationMode(DCOMPOSITION_BITMAP_INTERPOLATION_MODE_LINEAR)?;
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
      base_rect: PreviewSurfaceRect {
        height: 0.0,
        width: 0.0,
        x: 0.0,
        y: 0.0,
      },
      buffer_size: (2, 2),
      clip,
      clip_edges: (0, 0, 2, 2),
      content_size: (2, 2),
      display_size: (2, 2),
      last_camera: None,
      last_composition: None,
      settings: None,
      magnifier: None,
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
  fn update_geometry(&self) -> windows::core::Result<()> {
    let content_width = self.content_size.0.max(1) as f32;
    let content_height = self.content_size.1.max(1) as f32;
    let display_width = self.display_size.0.max(1) as f32;
    let display_height = self.display_size.1.max(1) as f32;
    let (clip_left, clip_top, clip_right, clip_bottom) = self.clip_edges;
    // One uniform scale, so a composed canvas whose aspect no longer matches
    // the laid-out box is centred inside it rather than stretched across it.
    // Aspects agree in the steady state and this is then exactly the
    // per-axis mapping; it only bites in the window between a re-composed
    // canvas and the layout that catches up with it.
    let scale = (display_width / content_width).min(display_height / content_height);
    let inset_x = (display_width - content_width * scale) / 2.0;
    let inset_y = (display_height - content_height * scale) / 2.0;
    unsafe {
      self.visual.SetOffsetX2(self.position.0 as f32 + inset_x)?;
      self.visual.SetOffsetY2(self.position.1 as f32 + inset_y)?;
      self.scale_transform.SetScaleX2(scale)?;
      self.scale_transform.SetScaleY2(scale)?;
      self.clip.SetLeft2((clip_left as f32 - inset_x) / scale)?;
      self.clip.SetTop2((clip_top as f32 - inset_y) / scale)?;
      self.clip.SetRight2((clip_right as f32 - inset_x) / scale)?;
      self
        .clip
        .SetBottom2((clip_bottom as f32 - inset_y) / scale)?;
    }
    Ok(())
  }

  fn hide(&self) {
    let _ = unsafe { self.visual.SetOffsetX2(-100_000.0) };
  }
}

fn set_pane_geometry(
  pane: &mut Pane,
  viewport: PreviewSurfaceRect,
  rect: PreviewSurfaceRect,
  scale: f64,
  defer_resize: bool,
) {
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
  if defer_resize {
    pane.pending_geometry = true;
  } else {
    // No present is on the way, so this geometry has to reach the compositor
    // now - a pan that parked itself would leave the pane behind the DOM
    // until something happened to compose a frame. Applying a box whose
    // aspect has outrun the buffer is safe: `update_geometry` centres the
    // composition inside it instead of stretching it across it.
    pane.pending_geometry = false;
    let _ = pane.update_geometry();
  }
}

/// Where the pane's composed canvas actually is inside the box it was laid
/// out in. A committed canvas resize keeps arriving from the DOM at the
/// session's source aspect, and the composition is centred inside it (see
/// `Pane::update_geometry`), so selection geometry has to follow the canvas
/// rather than the box to stay on the pixels the user sees.
fn pane_canvas_rect(pane: &Pane, owns_geometry: bool) -> PreviewSurfaceRect {
  // A live Frame resize already places the canvas itself, so nothing there
  // needs fitting - and fitting against the composition that is still one
  // present behind would make the overlay flicker for the whole drag.
  match pane.settings.as_ref().filter(|_| !owns_geometry) {
    Some(settings) => aspect_fit_rect(pane.base_rect, (settings.width, settings.height)),
    None => pane.base_rect,
  }
}

fn display_selection(
  state: &SurfaceState,
  selection: PreviewSelection,
) -> Option<PreviewSurfaceRect> {
  // A pane the last layout hid (the camera while it is baked into the
  // primary) has no display rect: its stale box must not select or hit-test,
  // matching the Metal backend's active-pane check.
  let pane = state
    .panes
    .get(selection.pane_index as usize)?
    .as_ref()
    .filter(|pane| pane.seen)?;
  let pane = state.workspace_transform.apply(
    state.viewport,
    pane_canvas_rect(pane, state.frame_resize.is_some()),
  );
  Some(PreviewSurfaceRect {
    x: pane.x + selection.x * pane.width,
    y: pane.y + selection.y * pane.height,
    width: selection.width * pane.width,
    height: selection.height * pane.height,
  })
}

fn update_magnifier(state: &mut SurfaceState) {
  let Some(ActiveGesture::Selection(gesture)) = state.gesture else {
    for pane in state.panes.iter_mut().flatten() {
      pane.magnifier = None;
    }
    return;
  };
  let show = gesture.operation == SelectionGestureOperation::CropResize;
  for pane in state.panes.iter_mut().flatten() {
    pane.magnifier = None;
  }
  if !show {
    return;
  }
  let Some(selection) = state.selection else {
    return;
  };
  let owns_geometry = state.frame_resize.is_some();
  let Some(pane) = state
    .panes
    .get_mut(selection.pane_index as usize)
    .and_then(Option::as_mut)
  else {
    return;
  };
  let rect = state
    .workspace_transform
    .apply(state.viewport, pane_canvas_rect(pane, owns_geometry));
  let Some(settings) = pane.settings.as_ref() else {
    return;
  };
  let display_point = if gesture.operation == SelectionGestureOperation::CropResize {
    let frame = PreviewSurfaceRect {
      x: rect.x + selection.x * rect.width,
      y: rect.y + selection.y * rect.height,
      width: selection.width * rect.width,
      height: selection.height * rect.height,
    };
    (
      if gesture.edges & 1 != 0 {
        frame.x
      } else if gesture.edges & 2 != 0 {
        frame.x + frame.width
      } else {
        state.last_pointer.0
      },
      if gesture.edges & 4 != 0 {
        frame.y
      } else if gesture.edges & 8 != 0 {
        frame.y + frame.height
      } else {
        state.last_pointer.1
      },
    )
  } else {
    state.last_pointer
  };
  let x = ((display_point.0 - rect.x) / rect.width * settings.width as f64) as f32;
  let y = ((display_point.1 - rect.y) / rect.height * settings.height as f64) as f32;
  let diameter = (96.0 * settings.width as f64 / rect.width.max(1.0)) as f32;
  let sample_camera = state.camera_source.is_some() && selection.layer_id != selection.pane_index;
  let luminance =
    state.backdrop[0] * 0.2126 + state.backdrop[1] * 0.7152 + state.backdrop[2] * 0.0722;
  pane.magnifier = Some(CropMagnifier {
    display_box: [
      (display_point.0 - 48.0) as f32,
      (display_point.1 - 48.0) as f32,
      96.0,
      96.0,
    ],
    // A 40-source-pixel window fills a 96-DIP native overlay.
    geometry: [x, y, diameter, diameter / 40.0],
    options: [
      if sample_camera { 1.0 } else { 0.0 },
      gesture.edges as f32,
      if luminance > 0.5 { 1.0 } else { 0.0 },
      0.0,
    ],
  });
}

fn redraw_magnifier(inner: &std::sync::Arc<SurfaceInner>, state: &mut SurfaceState) {
  let Some(selection) = state.selection else {
    return;
  };
  let camera_source = state.camera_source.clone();
  let Some(pane) = state
    .panes
    .get_mut(selection.pane_index as usize)
    .and_then(Option::as_mut)
  else {
    return;
  };
  let (Some(settings), Some(composition)) = (pane.settings.clone(), pane.last_composition) else {
    return;
  };
  // Redraw exactly what the last present composed: dropping a baked camera
  // here would blank it in crop mode and flicker it during gestures.
  let camera = match (pane.last_camera, camera_source.as_ref()) {
    (Some((geometry, drop_shadow, camera_on_top)), Some(source)) => {
      Some((source, geometry, drop_shadow, camera_on_top))
    }
    (Some(_), None) => return,
    (None, _) => None,
  };
  let surface = RecordingPreviewSurface {
    inner: std::sync::Arc::clone(inner),
  };
  let _ = surface.present_cached_source_with_camera(pane, &settings, composition, camera);
}

fn radius_point(frame: PreviewSurfaceRect, radius_percent: f64) -> (f64, f64) {
  let offset =
    frame.width.min(frame.height) * radius_percent.clamp(0.0, 50.0) / 100.0 * 0.55 + 10.0;
  (frame.x + offset, frame.y + offset)
}

fn shared_selection_hit(state: &SurfaceState, point: (f64, f64)) -> Option<(PreviewSelection, u8)> {
  let mut selections = state.selection_targets.clone();
  if let Some(current) = state.selection {
    if let Some(target) = selections
      .iter_mut()
      .find(|target| target.pane_index == current.pane_index && target.layer_id == current.layer_id)
    {
      *target = current;
    } else {
      selections.push(current);
    }
  }
  let targets = selections
    .iter()
    .enumerate()
    .filter_map(|(index, selection)| {
      let rect = display_selection(state, *selection)?;
      Some(DisplayTarget {
        id: (u64::from(selection.pane_index) << 32) | u64::from(selection.layer_id),
        rect: DisplayRect {
          x: rect.x,
          y: rect.y,
          width: rect.width,
          height: rect.height,
        },
        radius_enabled: u8::from(selection.crop_mode == 0 && selection.radius_disabled == 0),
        radius_percent: selection.radius_percent,
        z_order: index as i32,
        selected: u8::from(state.selection.is_some_and(|current| {
          current.pane_index == selection.pane_index && current.layer_id == selection.layer_id
        })),
        visible: 1,
      })
    })
    .collect::<Vec<_>>();
  let hit = hit_test_display(&targets, point, 8.0)?;
  let pane_index = (hit.target_id >> 32) as u32;
  let layer_id = hit.target_id as u32;
  selections
    .into_iter()
    .find(|selection| selection.pane_index == pane_index && selection.layer_id == layer_id)
    .map(|selection| (selection, hit.handle))
}

fn shared_handle_edges(handle: u8) -> u32 {
  match handle {
    1 => 4,
    2 => 8,
    3 => 2,
    4 => 1,
    5 => 2 | 4,
    6 => 1 | 4,
    7 => 2 | 8,
    8 => 1 | 8,
    _ => 0,
  }
}

fn selection_pane_rect(state: &SurfaceState, selection: PreviewSelection) -> PreviewSurfaceRect {
  state
    .panes
    .get(selection.pane_index as usize)
    .and_then(Option::as_ref)
    .map_or(
      PreviewSurfaceRect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
      },
      |pane| pane_canvas_rect(pane, state.frame_resize.is_some()),
    )
}

fn draw_selection(inner: &SurfaceInner, state: &SurfaceState) {
  let scale = state.scale.max(0.1);
  let (viewport_x, viewport_right) =
    window::scaled_edges(state.viewport.x, state.viewport.width, scale);
  let (viewport_y, viewport_bottom) =
    window::scaled_edges(state.viewport.y, state.viewport.height, scale);
  let display = (state.editor_active && state.selection_visible)
    .then(|| {
      state
        .selection
        .and_then(|selection| display_selection(state, selection).map(|rect| (selection, rect)))
        .map(|(selection, rect)| {
          let (x, right) = window::scaled_edges(rect.x, rect.width, scale);
          let (y, bottom) = window::scaled_edges(rect.y, rect.height, scale);
          let frame = [x as f32, y as f32, (right - x) as f32, (bottom - y) as f32];
          let radius = if selection.crop_mode == 0 && selection.radius_disabled == 0 {
            let radius = radius_point(rect, selection.radius_percent);
            [(radius.0 * scale) as f32, (radius.1 * scale) as f32]
          } else {
            [f32::NAN, f32::NAN]
          };
          (frame, radius)
        })
    })
    .flatten();
  let frame = display.map(|value| value.0);
  let radius = display.map(|value| value.1);
  let crop_image = display.and_then(|_| {
    let selection = state.selection?;
    (selection.crop_mode != 0).then_some(())?;
    let image = state
      .panes
      .get(selection.pane_index as usize)
      .and_then(Option::as_ref)
      .map(|pane| {
        state.workspace_transform.apply(
          state.viewport,
          pane_canvas_rect(pane, state.frame_resize.is_some()),
        )
      })?;
    Some([
      ((image.x + selection.image_x * image.width) * scale) as f32,
      ((image.y + selection.image_y * image.height) * scale) as f32,
      (selection.image_width * image.width * scale) as f32,
      (selection.image_height * image.height * scale) as f32,
    ])
  });
  let guides = display.and_then(|_| {
    let selection = state.selection?;
    let pane = state.panes.get(selection.pane_index as usize)?.as_ref()?;
    let pane = state.workspace_transform.apply(
      state.viewport,
      pane_canvas_rect(pane, state.frame_resize.is_some()),
    );
    let x = state
      .selection_snap_guide_x
      .map(|guide| ((pane.x + guide.guide * pane.width) * scale) as f32);
    let y = state
      .selection_snap_guide_y
      .map(|guide| ((pane.y + guide.guide * pane.height) * scale) as f32);
    Some((
      x,
      y,
      state
        .selection_snap_guide_x
        .map_or(false, |guide| guide.object),
      state
        .selection_snap_guide_y
        .map_or(false, |guide| guide.object),
    ))
  });
  let luminance =
    state.backdrop[0] * 0.2126 + state.backdrop[1] * 0.7152 + state.backdrop[2] * 0.0722;
  let magnifier_box = state
    .selection
    .and_then(|selection| state.panes.get(selection.pane_index as usize))
    .and_then(Option::as_ref)
    .and_then(|pane| pane.magnifier)
    .map(|magnifier| {
      let [x, y, width, height] = magnifier.display_box;
      [
        x * scale as f32,
        y * scale as f32,
        width * scale as f32,
        height * scale as f32,
      ]
    });
  if let Ok(mut overlay) = inner.gpu.selection.lock() {
    let _ = overlay.draw(
      &inner.gpu.device,
      &inner.gpu.context,
      (
        (viewport_right - viewport_x).max(2) as u32,
        (viewport_bottom - viewport_y).max(2) as u32,
      ),
      frame,
      radius.filter(|point| point[0].is_finite() && point[1].is_finite()),
      crop_image,
      guides,
      magnifier_box,
      luminance > 0.5,
    );
  }
}

fn clear_selection_snap_guides(state: &mut SurfaceState) {
  state.selection_snap_guide_x = None;
  state.selection_snap_guide_y = None;
}

fn selection_snap_targets(
  state: &SurfaceState,
  start: PreviewSelection,
  horizontal: bool,
) -> Vec<(u32, f64, f64)> {
  let Some(start_pane) = state
    .panes
    .get(start.pane_index as usize)
    .and_then(Option::as_ref)
  else {
    return Vec::new();
  };
  let same_frame = |target: &PreviewSelection| {
    state
      .panes
      .get(target.pane_index as usize)
      .and_then(Option::as_ref)
      .is_some_and(|pane| {
        let first = start_pane.base_rect;
        let second = pane.base_rect;
        (first.x - second.x).abs() < 0.000_001
          && (first.y - second.y).abs() < 0.000_001
          && (first.width - second.width).abs() < 0.000_001
          && (first.height - second.height).abs() < 0.000_001
      })
  };
  state
    .selection_targets
    .iter()
    .filter(|target| same_frame(target))
    .map(|target| {
      if horizontal {
        (target.layer_id, target.x, target.width)
      } else {
        (target.layer_id, target.y, target.height)
      }
    })
    .collect()
}

fn cursor_for_state(state: &SurfaceState, point: (f64, f64)) -> editor::CursorKind {
  let Some((selection, handle)) = shared_selection_hit(state, point) else {
    return editor::CursorKind::Arrow;
  };
  if handle == 0 && selection.layer_id == FRAME_LAYER_ID {
    return editor::CursorKind::Arrow;
  }
  let edges = shared_handle_edges(handle);
  match handle {
    0 => editor::CursorKind::Move,
    9 => editor::CursorKind::ResizeNwse,
    _ if edges == 1 || edges == 2 => editor::CursorKind::ResizeHorizontal,
    _ if edges == 4 || edges == 8 => editor::CursorKind::ResizeVertical,
    _ if edges == (1 | 4) || edges == (2 | 8) => editor::CursorKind::ResizeNwse,
    _ => editor::CursorKind::ResizeNesw,
  }
}

fn union_rect(left: PreviewSurfaceRect, right: PreviewSurfaceRect) -> PreviewSurfaceRect {
  let x = left.x.min(right.x);
  let y = left.y.min(right.y);
  let right_edge = (left.x + left.width).max(right.x + right.width);
  let bottom = (left.y + left.height).max(right.y + right.height);
  PreviewSurfaceRect {
    x,
    y,
    width: right_edge - x,
    height: bottom - y,
  }
}

/// Re-flows the sibling panes around the one a Frame resize is dragging, the
/// Windows counterpart of `reflow_recording_workspace_panes`: the row keeps
/// its gesture-start gaps and side ordering while every pane stays centred on
/// the row, so growing one canvas pushes its neighbours instead of
/// overlapping them.
fn reflow_workspace_panes(
  starts: &[(usize, PreviewSurfaceRect)],
  selected: usize,
  resized: PreviewSurfaceRect,
) -> Vec<(usize, PreviewSurfaceRect)> {
  let mut order = (0..starts.len()).collect::<Vec<_>>();
  order.sort_by(|left, right| {
    starts[*left]
      .1
      .x
      .partial_cmp(&starts[*right].1.x)
      .unwrap_or(std::cmp::Ordering::Equal)
  });
  let mut next = starts.to_vec();
  let Some(selected_position) = order
    .iter()
    .position(|position| starts[*position].0 == selected)
  else {
    return next;
  };
  next[order[selected_position]].1 = resized;
  let tallest = order.iter().fold(0.0_f64, |tallest, position| {
    tallest.max(next[*position].1.height)
  });
  let group_top = resized.y - (tallest - resized.height) / 2.0;
  for position in &order {
    next[*position].1.y = group_top + (tallest - next[*position].1.height) / 2.0;
  }
  for position in selected_position + 1..order.len() {
    let previous = order[position - 1];
    let index = order[position];
    let gap = starts[index].1.x - (starts[previous].1.x + starts[previous].1.width);
    next[index].1.x = next[previous].1.x + next[previous].1.width + gap;
  }
  for position in (0..selected_position).rev() {
    let index = order[position];
    let following = order[position + 1];
    let gap = starts[following].1.x - (starts[index].1.x + starts[index].1.width);
    next[index].1.x = next[following].1.x - gap - next[index].1.width;
  }
  next
}

/// Centres a composition of `content` aspect inside `rect`. A committed
/// canvas resize keeps arriving from the DOM at the session's fixed source
/// aspect for a layout or two; fitting rather than filling means the pane
/// shows the composed canvas whole instead of stretching it.
fn aspect_fit_rect(rect: PreviewSurfaceRect, content: (u32, u32)) -> PreviewSurfaceRect {
  let content_width = f64::from(content.0.max(1));
  let content_height = f64::from(content.1.max(1));
  let first = content_width * rect.height;
  let second = content_height * rect.width;
  let scale = first.max(second).max(1.0);
  if rect.width <= 0.0 || rect.height <= 0.0 || (first - second).abs() / scale < 0.005 {
    return rect;
  }
  let fit = (rect.width / content_width).min(rect.height / content_height);
  let width = content_width * fit;
  let height = content_height * fit;
  PreviewSurfaceRect {
    x: rect.x + (rect.width - width) / 2.0,
    y: rect.y + (rect.height - height) / 2.0,
    width,
    height,
  }
}

/// Re-expresses the resized workspace against a fresh centred fit without
/// moving a single displayed pixel, mirroring
/// `rebase_recording_workspace_fit`. `start` supplies the gesture's immutable
/// transform, so `displayed` is where the panes actually are on screen; only
/// the fit-relative zoom/pan representation changes, which is what makes the
/// toolbar percentage follow the drag and the commit land without a jump.
fn rebase_workspace_fit(state: &mut SurfaceState, start: &FrameResizeStart) {
  let active = state
    .panes
    .iter()
    .enumerate()
    .filter_map(|(index, pane)| {
      pane
        .as_ref()
        .filter(|pane| pane.seen)
        .map(|pane| (index, pane.base_rect))
    })
    .collect::<Vec<_>>();
  let Some((_, first)) = active.first().copied() else {
    return;
  };
  let bounds = active
    .iter()
    .skip(1)
    .fold(first, |bounds, (_, rect)| union_rect(bounds, *rect));
  let start_bounds = active
    .iter()
    .map(|(index, rect)| {
      start
        .pane_rects
        .iter()
        .find(|(start_index, _)| start_index == index)
        .map_or(*rect, |(_, start_rect)| *start_rect)
    })
    .reduce(union_rect)
    .unwrap_or(bounds);
  let first_display = start.transform.apply(state.viewport, first);
  let displayed = active
    .iter()
    .skip(1)
    .fold(first_display, |bounds, (_, rect)| {
      union_rect(bounds, start.transform.apply(state.viewport, *rect))
    });
  if let Some((width, height)) = start.natural_size {
    state.workspace_natural_size = Some((
      ((f64::from(width) * bounds.width / start_bounds.width.max(1.0)).round()).max(1.0) as u32,
      ((f64::from(height) * bounds.height / start_bounds.height.max(1.0)).round()).max(1.0) as u32,
    ));
  }
  // Pane rects and `WorkspaceTransform::apply` are viewport-relative, so the
  // displayed union and the fit stay in that space; adding the viewport origin
  // here would shift every pane by it on each move.
  let rebased = rebase_display_fit(
    (state.viewport.width, state.viewport.height),
    DisplayRect {
      x: displayed.x,
      y: displayed.y,
      width: displayed.width,
      height: displayed.height,
    },
    8.0,
  );
  let fit = PreviewSurfaceRect {
    x: rebased.fit.x,
    y: rebased.fit.y,
    width: rebased.fit.width,
    height: rebased.fit.height,
  };
  let scale_x = fit.width / bounds.width.max(1.0);
  let scale_y = fit.height / bounds.height.max(1.0);
  for (index, rect) in active {
    if let Some(pane) = state.panes.get_mut(index).and_then(Option::as_mut) {
      pane.base_rect = PreviewSurfaceRect {
        x: fit.x + (rect.x - bounds.x) * scale_x,
        y: fit.y + (rect.y - bounds.y) * scale_y,
        width: rect.width * scale_x,
        height: rect.height * scale_y,
      };
    }
  }
  state.workspace_transform.zoom = rebased.zoom;
  state.workspace_transform.pan_x = rebased.pan_x;
  state.workspace_transform.pan_y = rebased.pan_y;
}

/// Publishes the workspace transform to every pane. `defer_geometry` parks the
/// pane boxes instead of committing them: a canvas resize changes the box and
/// the composition together, and the re-composed still that follows this call
/// publishes the parked geometry with its own present, so the pane never shows
/// the previous canvas letterboxed into the new box for a frame. The selection
/// overlay is always redrawn immediately - it tracks the box, not the pixels.
fn apply_workspace_transform(inner: &SurfaceInner, state: &mut SurfaceState, defer_geometry: bool) {
  let transform = state.workspace_transform;
  let viewport = state.viewport;
  let scale = state.scale;
  for pane in state.panes.iter_mut().flatten().filter(|pane| pane.seen) {
    let rect = transform.apply(viewport, pane.base_rect);
    set_pane_geometry(pane, viewport, rect, scale, defer_geometry);
  }
  draw_selection(inner, state);
  let _ = unsafe { inner.gpu.composition.Commit() };
}

fn emit_transform(inner: &SurfaceInner, zoom: f64) {
  if let Ok(mut callbacks) = inner.callbacks.lock() {
    if let Some(callback) = callbacks.transform.as_mut() {
      callback(zoom * 100.0);
    }
  }
}

fn emit_selection(inner: &SurfaceInner, selection: Option<u32>) {
  if let Ok(mut callbacks) = inner.callbacks.lock() {
    if let Some(callback) = callbacks.selection.as_mut() {
      callback(selection);
    }
  }
}

fn emit_gesture(inner: &SurfaceInner, phase: SelectionGesturePhase, gesture: EditorGesture) {
  if let Ok(mut callbacks) = inner.callbacks.lock() {
    if let Some(callback) = callbacks.gesture.as_mut() {
      // Frame gestures address the pane, not the sentinel frame layer id,
      // matching the Metal backend's `emit_selection_gesture`.
      let layer_id = if matches!(
        gesture.operation,
        SelectionGestureOperation::FrameResize | SelectionGestureOperation::FrameRadius
      ) {
        gesture.selection_start.pane_index
      } else {
        gesture.selection_start.layer_id
      };
      callback(
        phase,
        layer_id,
        gesture.operation,
        gesture.edges,
        gesture.last_scale,
        gesture.last_delta.0,
        gesture.last_delta.1,
      );
    }
  }
}

pub(super) fn refresh_editor_cursor() {
  let Some(Ok(inner)) = PREVIEW_SURFACE.get() else {
    return;
  };
  let kind = inner
    .state
    .lock()
    .ok()
    .map(|state| match state.gesture {
      Some(ActiveGesture::Pan { .. }) => editor::CursorKind::Move,
      Some(ActiveGesture::Selection(gesture))
        if matches!(
          gesture.operation,
          SelectionGestureOperation::Move | SelectionGestureOperation::CropMove
        ) =>
      {
        editor::CursorKind::Move
      }
      Some(ActiveGesture::Selection(gesture))
        if gesture.operation == SelectionGestureOperation::Radius =>
      {
        editor::CursorKind::ResizeNwse
      }
      Some(ActiveGesture::Selection(gesture)) => {
        let edges = gesture.edges;
        if edges == 1 || edges == 2 {
          editor::CursorKind::ResizeHorizontal
        } else if edges == 4 || edges == 8 {
          editor::CursorKind::ResizeVertical
        } else if edges == (1 | 4) || edges == (2 | 8) {
          editor::CursorKind::ResizeNwse
        } else {
          editor::CursorKind::ResizeNesw
        }
      }
      _ => cursor_for_state(&state, state.last_pointer),
    })
    .unwrap_or(editor::CursorKind::Arrow);
  editor::EditorWindow::set_cursor(kind);
}

fn handle_editor_input(input: editor::Input) {
  let Some(Ok(inner)) = PREVIEW_SURFACE.get() else {
    return;
  };
  let scale = inner
    .state
    .lock()
    .ok()
    .map_or(1.0, |state| state.scale.max(0.1));
  let logical = |x: f64, y: f64| (x / scale, y / scale);
  match input {
    editor::Input::Down {
      centered: _,
      x,
      y,
      snapping: _,
    } => {
      let point = logical(x, y);
      let mut selected = None;
      let mut began = None;
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        let shared = shared_selection_hit(&state, point);
        let inactive_frame_target = shared
          .filter(|(selection, _)| {
            selection.layer_id == FRAME_LAYER_ID
              && state.selection.is_none_or(|current| {
                current.pane_index != selection.pane_index || current.layer_id != selection.layer_id
              })
          })
          .map(|hit| hit.0);
        let radius = shared.filter(|(_, handle)| *handle == 9).map(|hit| hit.0);
        let handle = shared
          .filter(|(_, handle)| (1..=8).contains(handle))
          .map(|(selection, handle)| (selection, shared_handle_edges(handle)));
        let target = shared
          .filter(|(selection, handle)| *handle == 0 && selection.layer_id != FRAME_LAYER_ID)
          .map(|hit| hit.0);
        let frame_target = shared
          .filter(|(selection, handle)| *handle == 0 && selection.layer_id == FRAME_LAYER_ID)
          .map(|hit| hit.0);
        if let Some(target) = inactive_frame_target {
          state.selection = Some(target);
          state.gesture = None;
          clear_selection_snap_guides(&mut state);
          selected = Some(Some(target.pane_index));
          draw_selection(inner, &state);
        } else if let Some(selection) = radius {
          let changed = state.selection.is_none_or(|current| {
            current.pane_index != selection.pane_index || current.layer_id != selection.layer_id
          });
          state.selection = Some(selection);
          let gesture = EditorGesture {
            edges: 0,
            last_delta: (0.0, 0.0),
            last_scale: selection.radius_percent,
            operation: if selection.layer_id == FRAME_LAYER_ID {
              SelectionGestureOperation::FrameRadius
            } else {
              SelectionGestureOperation::Radius
            },
            pane_start: selection_pane_rect(&state, selection),
            pointer_start: point,
            selection_start: selection,
          };
          state.gesture = Some(ActiveGesture::Selection(gesture));
          if changed {
            selected = Some(Some(if selection.layer_id == FRAME_LAYER_ID {
              selection.pane_index
            } else {
              selection.layer_id
            }));
          }
          began = Some(gesture);
          draw_selection(inner, &state);
        } else if let Some((selection, edges)) = handle {
          let changed = state.selection.is_none_or(|current| {
            current.pane_index != selection.pane_index || current.layer_id != selection.layer_id
          });
          state.selection = Some(selection);
          let gesture = EditorGesture {
            edges,
            last_delta: (0.0, 0.0),
            last_scale: 1.0,
            operation: if selection.layer_id == FRAME_LAYER_ID {
              SelectionGestureOperation::FrameResize
            } else if selection.crop_mode != 0 {
              SelectionGestureOperation::CropResize
            } else {
              SelectionGestureOperation::Resize
            },
            pane_start: selection_pane_rect(&state, selection),
            pointer_start: point,
            selection_start: selection,
          };
          if gesture.operation == SelectionGestureOperation::FrameResize {
            // Remember the transform that belongs to the canvas size being
            // left behind, so undoing back to it restores that zoom.
            if let Some(size) = state.workspace_natural_size {
              let transform = state.workspace_transform;
              state.workspace_transforms.insert(size, transform);
            }
            state.frame_resize = Some(FrameResizeStart {
              natural_size: state.workspace_natural_size,
              pane_rects: state
                .panes
                .iter()
                .enumerate()
                .filter_map(|(index, pane)| {
                  pane
                    .as_ref()
                    .filter(|pane| pane.seen)
                    .map(|pane| (index, pane_canvas_rect(pane, false)))
                })
                .collect(),
              transform: state.workspace_transform,
            });
          }
          state.gesture = Some(ActiveGesture::Selection(gesture));
          if changed {
            selected = Some(Some(if selection.layer_id == FRAME_LAYER_ID {
              selection.pane_index
            } else {
              selection.layer_id
            }));
          }
          began = Some(gesture);
          draw_selection(inner, &state);
        } else if let Some(target) = frame_target {
          let changed = state.selection.is_none_or(|current| {
            current.pane_index != target.pane_index || current.layer_id != target.layer_id
          });
          state.selection = Some(target);
          state.gesture = None;
          clear_selection_snap_guides(&mut state);
          if changed {
            selected = Some(Some(target.pane_index));
          }
          draw_selection(inner, &state);
        } else if let Some(target) = target {
          let changed = state.selection.is_none_or(|current| {
            current.pane_index != target.pane_index || current.layer_id != target.layer_id
          });
          let selection = if changed {
            target
          } else {
            state.selection.unwrap_or(target)
          };
          state.selection = Some(selection);
          let gesture = EditorGesture {
            edges: 0,
            last_delta: (0.0, 0.0),
            last_scale: 1.0,
            operation: if selection.crop_mode != 0 {
              SelectionGestureOperation::CropMove
            } else {
              SelectionGestureOperation::Move
            },
            pane_start: selection_pane_rect(&state, selection),
            pointer_start: point,
            selection_start: selection,
          };
          state.gesture = Some(ActiveGesture::Selection(gesture));
          if changed {
            selected = Some(Some(if target.layer_id == FRAME_LAYER_ID {
              target.pane_index
            } else {
              target.layer_id
            }));
          }
          began = Some(gesture);
          draw_selection(inner, &state);
        } else {
          state.gesture = Some(ActiveGesture::Pan {
            pointer_start: point,
            transform_start: state.workspace_transform,
          });
          draw_selection(inner, &state);
        }
      }
      if let Some(selection) = selected {
        emit_selection(inner, selection);
      }
      if let Some(gesture) = began {
        emit_gesture(inner, SelectionGesturePhase::Begin, gesture);
      }
      refresh_editor_cursor();
    }
    editor::Input::Move {
      centered,
      x,
      y,
      pressed,
      snapping,
    } => {
      let point = logical(x, y);
      let mut update = None;
      let mut zoom = None;
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        if pressed {
          match state.gesture {
            Some(ActiveGesture::Pan {
              pointer_start,
              transform_start,
            }) => {
              state.workspace_transform.pan_x = transform_start.pan_x + point.0 - pointer_start.0;
              state.workspace_transform.pan_y = transform_start.pan_y + point.1 - pointer_start.1;
              apply_workspace_transform(inner, &mut state, false);
            }
            Some(ActiveGesture::Selection(mut gesture)) => {
              let owns_geometry = state.frame_resize.is_some();
              let pane = state
                .panes
                .get(gesture.selection_start.pane_index as usize)
                .and_then(Option::as_ref)
                .map(|pane| pane_canvas_rect(pane, owns_geometry));
              if let Some(pane) = pane {
                let dx = (point.0 - gesture.pointer_start.0)
                  / (pane.width * state.workspace_transform.zoom).max(1.0);
                let dy = (point.1 - gesture.pointer_start.1)
                  / (pane.height * state.workspace_transform.zoom).max(1.0);
                let mut selection = gesture.selection_start;
                if gesture.operation == SelectionGestureOperation::FrameResize {
                  clear_selection_snap_guides(&mut state);
                  let start = gesture.pane_start;
                  // Pointer travel is display points; pane rects are pre-zoom
                  // workspace points. The gesture's *starting* zoom converts
                  // between them for the whole drag - the live zoom is
                  // rebased on every move, and feeding that back would make
                  // the canvas chase the pointer.
                  let start_zoom = state
                    .frame_resize
                    .as_ref()
                    .map_or(state.workspace_transform.zoom, |start| start.transform.zoom);
                  let raw_x = (point.0 - gesture.pointer_start.0) / start_zoom.max(0.0001);
                  let raw_y = (point.1 - gesture.pointer_start.1) / start_zoom.max(0.0001);
                  let edges = gesture.edges & !CENTERED_RESIZE_EDGE;
                  let mut left = start.x;
                  let mut right = start.x + start.width;
                  let mut top = start.y;
                  let mut bottom = start.y + start.height;
                  if edges & 1 != 0 {
                    let movement = raw_x.min(if centered {
                      (start.width - 36.0) / 2.0
                    } else {
                      start.width - 36.0
                    });
                    left += movement;
                    if centered {
                      right -= movement;
                    }
                  } else if edges & 2 != 0 {
                    let movement = raw_x.max(if centered {
                      -(start.width - 36.0) / 2.0
                    } else {
                      36.0 - start.width
                    });
                    right += movement;
                    if centered {
                      left -= movement;
                    }
                  }
                  if edges & 4 != 0 {
                    let movement = raw_y.min(if centered {
                      (start.height - 36.0) / 2.0
                    } else {
                      start.height - 36.0
                    });
                    top += movement;
                    if centered {
                      bottom -= movement;
                    }
                  } else if edges & 8 != 0 {
                    let movement = raw_y.max(if centered {
                      -(start.height - 36.0) / 2.0
                    } else {
                      36.0 - start.height
                    });
                    bottom += movement;
                    if centered {
                      top -= movement;
                    }
                  }
                  let resized = PreviewSurfaceRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                  };
                  let selected = gesture.selection_start.pane_index as usize;
                  // Re-derive the whole workspace from the gesture's starts:
                  // the dragged canvas, then its siblings re-flowed around
                  // it, then one rebase that re-expresses zoom/pan so none of
                  // it moves on screen. Without the starts the row would
                  // accumulate its own re-flow each move.
                  if let Some(start_state) = state.frame_resize.take() {
                    let reflowed =
                      reflow_workspace_panes(&start_state.pane_rects, selected, resized);
                    for (index, rect) in reflowed {
                      if let Some(pane) = state.panes.get_mut(index).and_then(Option::as_mut) {
                        pane.base_rect = rect;
                      }
                    }
                    rebase_workspace_fit(&mut state, &start_state);
                    state.frame_resize = Some(start_state);
                    zoom = Some(state.workspace_transform.zoom);
                  } else if let Some(pane) = state.panes.get_mut(selected).and_then(Option::as_mut)
                  {
                    pane.base_rect = resized;
                  }
                  gesture.edges = edges | if centered { CENTERED_RESIZE_EDGE } else { 0 };
                  gesture.last_delta =
                    (raw_x / start.width.max(1.0), raw_y / start.height.max(1.0));
                  gesture.last_scale = 1.0;
                  // The gesture emitted below re-composes this canvas
                  // synchronously, and that present publishes the boxes: one
                  // commit per input, with pixels and geometry agreeing.
                  //
                  // The composed canvas and this box are derived from the
                  // same pointer travel (`resize_recording_frame` versus the
                  // edge maths above), so they agree except where the shared
                  // model's own bounds bite - `FRAME_MIN_SIZE` (64 output px)
                  // and `FRAME_MAX_AREA`, neither of which the local 36-point
                  // minimum expresses, plus integer rounding of the output
                  // size. At those extremes `update_geometry`'s uniform fit
                  // centres the canvas in the box, which is the graceful
                  // outcome; nothing accumulates, because every move
                  // re-derives the box from the gesture's starts.
                  apply_workspace_transform(inner, &mut state, true);
                } else if matches!(
                  gesture.operation,
                  SelectionGestureOperation::Radius | SelectionGestureOperation::FrameRadius
                ) {
                  clear_selection_snap_guides(&mut state);
                  let frame = display_selection(&state, gesture.selection_start).unwrap_or(
                    PreviewSurfaceRect {
                      height: 1.0,
                      width: 1.0,
                      x: 0.0,
                      y: 0.0,
                    },
                  );
                  let shortest = frame.width.min(frame.height).max(1.0);
                  let radius = (((point.0 - frame.x) + (point.1 - frame.y)) / 2.0 - 10.0) / 0.55;
                  selection.radius_percent = (radius * 100.0 / shortest).clamp(0.0, 50.0);
                  gesture.last_scale = selection.radius_percent;
                } else if gesture.operation == SelectionGestureOperation::Move {
                  selection.x += dx;
                  selection.y += dy;
                  gesture.last_delta = (dx, dy);
                  if state.selection_snapping_enabled && snapping {
                    let targets_x = selection_snap_targets(&state, gesture.selection_start, true);
                    let targets_y = selection_snap_targets(&state, gesture.selection_start, false);
                    let horizontal = snapping::move_axis(
                      selection.x,
                      selection.width,
                      pane.width,
                      pane.height,
                      state.workspace_transform.zoom,
                      &targets_x,
                      selection.layer_id,
                    );
                    let vertical = snapping::move_axis(
                      selection.y,
                      selection.height,
                      pane.height,
                      pane.width,
                      state.workspace_transform.zoom,
                      &targets_y,
                      selection.layer_id,
                    );
                    if horizontal.found {
                      selection.x += horizontal.adjustment;
                    }
                    if vertical.found {
                      selection.y += vertical.adjustment;
                    }
                    state.selection_snap_guide_x = horizontal.found.then_some(horizontal);
                    state.selection_snap_guide_y = vertical.found.then_some(vertical);
                    gesture.last_delta = (
                      selection.x - gesture.selection_start.x,
                      selection.y - gesture.selection_start.y,
                    );
                  } else {
                    clear_selection_snap_guides(&mut state);
                  }
                } else if gesture.operation == SelectionGestureOperation::CropMove {
                  clear_selection_snap_guides(&mut state);
                  let crop = NormalizedRect {
                    x: selection.x,
                    y: selection.y,
                    width: selection.width,
                    height: selection.height,
                  };
                  let image = NormalizedRect {
                    x: gesture.selection_start.image_x,
                    y: gesture.selection_start.image_y,
                    width: gesture.selection_start.image_width,
                    height: gesture.selection_start.image_height,
                  };
                  let next = apply_crop_move(crop, image, (dx, dy));
                  selection.x = next.x;
                  selection.y = next.y;
                  gesture.last_delta = (
                    selection.x - gesture.selection_start.x,
                    selection.y - gesture.selection_start.y,
                  );
                } else if gesture.operation == SelectionGestureOperation::CropResize {
                  clear_selection_snap_guides(&mut state);
                  let crop = NormalizedRect {
                    x: gesture.selection_start.x,
                    y: gesture.selection_start.y,
                    width: gesture.selection_start.width,
                    height: gesture.selection_start.height,
                  };
                  let image = NormalizedRect {
                    x: gesture.selection_start.image_x,
                    y: gesture.selection_start.image_y,
                    width: gesture.selection_start.image_width,
                    height: gesture.selection_start.image_height,
                  };
                  let next = apply_crop_resize(crop, image, gesture.edges, (dx, dy), false);
                  selection.x = next.x;
                  selection.y = next.y;
                  selection.width = next.width;
                  selection.height = next.height;
                  gesture.last_delta = (
                    if gesture.edges & 1 != 0 {
                      selection.x - gesture.selection_start.x
                    } else if gesture.edges & 2 != 0 {
                      selection.x + selection.width
                        - gesture.selection_start.x
                        - gesture.selection_start.width
                    } else {
                      0.0
                    },
                    if gesture.edges & 4 != 0 {
                      selection.y - gesture.selection_start.y
                    } else if gesture.edges & 8 != 0 {
                      selection.y + selection.height
                        - gesture.selection_start.y
                        - gesture.selection_start.height
                    } else {
                      0.0
                    },
                  );
                  gesture.last_scale = if gesture.selection_start.width.abs() > f64::EPSILON {
                    selection.width / gesture.selection_start.width
                  } else {
                    1.0
                  };
                } else {
                  let edges = gesture.edges;
                  let start = gesture.selection_start;
                  let anchor_x = if edges & 1 != 0 {
                    start.x + start.width
                  } else if edges & 2 != 0 {
                    start.x
                  } else {
                    start.x + start.width / 2.0
                  };
                  let anchor_y = if edges & 4 != 0 {
                    start.y + start.height
                  } else if edges & 8 != 0 {
                    start.y
                  } else {
                    start.y + start.height / 2.0
                  };
                  let handle_x = if edges & 1 != 0 {
                    start.x
                  } else if edges & 2 != 0 {
                    start.x + start.width
                  } else {
                    start.x + start.width / 2.0
                  };
                  let handle_y = if edges & 4 != 0 {
                    start.y
                  } else if edges & 8 != 0 {
                    start.y + start.height
                  } else {
                    start.y + start.height / 2.0
                  };
                  let vx = handle_x - anchor_x;
                  let vy = handle_y - anchor_y;
                  let denominator = vx * vx + vy * vy;
                  let factor = if denominator > 0.0 {
                    ((dx + vx) * vx + (dy + vy) * vy) / denominator
                  } else {
                    1.0
                  };
                  let minimum = (36.0
                    / (pane.width * state.workspace_transform.zoom * start.width).max(1.0))
                  .max(
                    36.0 / (pane.height * state.workspace_transform.zoom * start.height).max(1.0),
                  );
                  let factor = factor.clamp(minimum.min(8.0), 8.0);
                  let mut factor = factor;
                  if state.selection_snapping_enabled && snapping {
                    let targets_x = selection_snap_targets(&state, start, true);
                    let targets_y = selection_snap_targets(&state, start, false);
                    let horizontal = snapping::resize_axis(
                      anchor_x,
                      vx,
                      factor,
                      pane.width,
                      pane.height,
                      state.workspace_transform.zoom,
                      minimum,
                      8.0,
                      &targets_x,
                      start.layer_id,
                    );
                    let vertical = snapping::resize_axis(
                      anchor_y,
                      vy,
                      factor,
                      pane.height,
                      pane.width,
                      state.workspace_transform.zoom,
                      minimum,
                      8.0,
                      &targets_y,
                      start.layer_id,
                    );
                    let chosen = if horizontal.found
                      && (!vertical.found || horizontal.distance <= vertical.distance)
                    {
                      horizontal
                    } else {
                      vertical
                    };
                    if chosen.found {
                      factor = chosen.adjustment;
                    }
                    let x_difference = horizontal
                      .found
                      .then_some(
                        (horizontal.adjustment - factor).abs()
                          * vx.abs()
                          * pane.width
                          * state.workspace_transform.zoom,
                      )
                      .unwrap_or(f64::INFINITY);
                    let y_difference = vertical
                      .found
                      .then_some(
                        (vertical.adjustment - factor).abs()
                          * vy.abs()
                          * pane.height
                          * state.workspace_transform.zoom,
                      )
                      .unwrap_or(f64::INFINITY);
                    state.selection_snap_guide_x =
                      (horizontal.found && x_difference <= 0.5).then_some(horizontal);
                    state.selection_snap_guide_y =
                      (vertical.found && y_difference <= 0.5).then_some(vertical);
                  } else {
                    clear_selection_snap_guides(&mut state);
                  }
                  selection.x = anchor_x + (start.x - anchor_x) * factor;
                  selection.y = anchor_y + (start.y - anchor_y) * factor;
                  selection.width = start.width * factor;
                  selection.height = start.height * factor;
                  gesture.last_delta = (selection.x - start.x, selection.y - start.y);
                  gesture.last_scale = factor;
                }
                state.selection = Some(selection);
                state.gesture = Some(ActiveGesture::Selection(gesture));
                update_magnifier(&mut state);
                redraw_magnifier(inner, &mut state);
                draw_selection(inner, &state);
                let _ = unsafe { inner.gpu.composition.Commit() };
                update = Some(gesture);
              }
            }
            None => {}
          }
        } else {
          editor::EditorWindow::set_cursor(cursor_for_state(&state, point));
        }
      }
      // The toolbar percentage follows a live canvas resize: the rebase keeps
      // the pixels still by changing the zoom the workspace is expressed in.
      if let Some(zoom) = zoom {
        emit_transform(inner, zoom);
      }
      if let Some(gesture) = update {
        emit_gesture(inner, SelectionGesturePhase::Update, gesture);
      }
    }
    editor::Input::PanDown { x, y } => {
      let point = logical(x, y);
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        // A selection drag already in flight on the primary button keeps
        // going; the middle button only pans from rest.
        if state.gesture.is_none() {
          state.gesture = Some(ActiveGesture::Pan {
            pointer_start: point,
            transform_start: state.workspace_transform,
          });
        }
      }
      refresh_editor_cursor();
    }
    editor::Input::PanUp { x, y } => {
      let point = logical(x, y);
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        if matches!(state.gesture, Some(ActiveGesture::Pan { .. })) {
          state.gesture = None;
        }
        editor::EditorWindow::set_cursor(cursor_for_state(&state, point));
      }
    }
    editor::Input::Up { x, y } => {
      let point = logical(x, y);
      let mut ended = None;
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        if let Some(ActiveGesture::Selection(gesture)) = state.gesture.take() {
          if gesture.operation == SelectionGestureOperation::FrameResize {
            state.frame_resize = None;
            // The committed layout that follows carries the canvas size the
            // drag produced. It must adopt the rebased transform rather than
            // restore a remembered one, and record it as the transform that
            // belongs to that size.
            state.frame_resize_committed = true;
          }
          ended = Some(gesture);
        } else {
          state.gesture = None;
        }
        clear_selection_snap_guides(&mut state);
        update_magnifier(&mut state);
        redraw_magnifier(inner, &mut state);
        draw_selection(inner, &state);
        editor::EditorWindow::set_cursor(cursor_for_state(&state, point));
      }
      if let Some(gesture) = ended {
        emit_gesture(inner, SelectionGesturePhase::End, gesture);
      }
    }
    editor::Input::Cancel => {
      let mut cancelled = None;
      let mut cancelled_zoom = None;
      if let Ok(mut state) = inner.state.lock() {
        if let Some(ActiveGesture::Selection(gesture)) = state.gesture.take() {
          state.selection = Some(gesture.selection_start);
          if gesture.operation == SelectionGestureOperation::FrameResize {
            // Restore the whole workspace the drag re-flowed and rebased, not
            // just the pane under the pointer.
            if let Some(start) = state.frame_resize.take() {
              for (index, rect) in &start.pane_rects {
                if let Some(pane) = state.panes.get_mut(*index).and_then(Option::as_mut) {
                  pane.base_rect = *rect;
                }
              }
              state.workspace_transform = start.transform;
              state.workspace_natural_size = start.natural_size;
              cancelled_zoom = Some(start.transform.zoom);
            } else if let Some(pane) = state
              .panes
              .get_mut(gesture.selection_start.pane_index as usize)
              .and_then(Option::as_mut)
            {
              pane.base_rect = gesture.pane_start;
            }
            state.frame_resize_committed = false;
            // The pane still holds the canvas the drag composed, so the
            // restored boxes wait for the cancel gesture's re-composition of
            // the restored composition (and, failing that, the interactive
            // still the manager restarts) rather than letterboxing the
            // dragged canvas into them for a frame.
            apply_workspace_transform(inner, &mut state, true);
          }
          clear_selection_snap_guides(&mut state);
          update_magnifier(&mut state);
          redraw_magnifier(inner, &mut state);
          draw_selection(inner, &state);
          cancelled = Some(gesture);
        } else {
          state.gesture = None;
          clear_selection_snap_guides(&mut state);
        }
      }
      if let Some(zoom) = cancelled_zoom {
        emit_transform(inner, zoom);
      }
      if let Some(gesture) = cancelled {
        emit_gesture(inner, SelectionGesturePhase::Cancel, gesture);
      }
    }
    editor::Input::Wheel { x, y, delta } => {
      let point = logical(x, y);
      let mut zoom = None;
      if let Ok(mut state) = inner.state.lock() {
        state.last_pointer = point;
        let old = state.workspace_transform.zoom;
        let next = (old * (delta * 0.12).exp()).clamp(0.1, 16.0);
        let ratio = next / old;
        let center = (state.viewport.width / 2.0, state.viewport.height / 2.0);
        state.workspace_transform.pan_x =
          point.0 - center.0 - (point.0 - center.0 - state.workspace_transform.pan_x) * ratio;
        state.workspace_transform.pan_y =
          point.1 - center.1 - (point.1 - center.1 - state.workspace_transform.pan_y) * ratio;
        state.workspace_transform.zoom = next;
        apply_workspace_transform(inner, &mut state, false);
        zoom = Some(next);
      }
      if let Some(zoom) = zoom {
        emit_transform(inner, zoom);
      }
    }
    editor::Input::DoubleClick { .. } => {
      if let Ok(mut state) = inner.state.lock() {
        state.workspace_transform = WorkspaceTransform::default();
        apply_workspace_transform(inner, &mut state, false);
      }
      emit_transform(inner, 1.0);
    }
  }
}

/// Creates the native input child window on the thread that owns the host
/// HWND. Win32 queues a window's messages on its creating thread, and only the
/// event-loop thread pumps them, so an editor created on a worker thread
/// (Tauri's blocking pool, an IPC command thread) would never receive a mouse
/// message and every native gesture would be dead.
fn create_editor_on_owning_thread(
  window: &WebviewWindow,
  host: HWND,
) -> Result<editor::EditorWindow, String> {
  if unsafe { GetWindowThreadProcessId(host, None) } == unsafe { GetCurrentThreadId() } {
    // Already on the event-loop thread: create inline. Dispatching and then
    // blocking for the reply here would deadlock tao's loop against itself.
    return editor::EditorWindow::new(host);
  }
  // `HWND` is a raw pointer and so not `Send`; a window handle is just an
  // opaque process-wide token, safe to hand to the thread that owns it.
  struct HostHandle(HWND);
  unsafe impl Send for HostHandle {}

  let handle = HostHandle(host);
  let (sender, receiver) = std::sync::mpsc::channel();
  window
    .run_on_main_thread(move || {
      let handle = handle;
      let _ = sender.send(editor::EditorWindow::new(handle.0));
    })
    .map_err(|error| format!("The Windows preview editor could not be dispatched: {error}"))?;
  receiver
    .recv()
    .map_err(|_| "The Windows preview editor was never created on the main thread".to_owned())?
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
    pane.last_camera = camera
      .map(|(_, geometry, drop_shadow, camera_on_top)| (geometry, drop_shadow, camera_on_top));
    pane.settings = Some(settings.clone());
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
        unsafe {
          self
            .inner
            .gpu
            .context
            .ClearRenderTargetView(&view, &[0.0; 4])
        };
      }
    }
    self.inner.gpu.compositor.draw_with_camera(
      &self.inner.gpu.context,
      &target,
      source,
      settings,
      composition,
      camera,
      pane.magnifier,
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
        let editor = create_editor_on_owning_thread(window, host)?;
        let gpu = Gpu::new(host, editor.hwnd())?;
        Ok(std::sync::Arc::new(SurfaceInner {
          batch_depth: AtomicU32::new(0),
          callbacks: Mutex::new(EditorCallbacks::default()),
          editor,
          gpu,
          state: Mutex::new(SurfaceState {
            backdrop: [0.09, 0.09, 0.10, 1.0],
            camera_source: None,
            editor_active: false,
            frame_resize: None,
            frame_resize_committed: false,
            gesture: None,
            last_pointer: (0.0, 0.0),
            panes: Vec::new(),
            primary_composition: None,
            scale: 1.0,
            selection: None,
            selection_visible: true,
            selection_snapping_enabled: false,
            selection_snap_guide_x: None,
            selection_snap_guide_y: None,
            selection_targets: Vec::new(),
            viewport: PreviewSurfaceRect {
              height: 0.0,
              width: 0.0,
              x: 0.0,
              y: 0.0,
            },
            workspace_transform: WorkspaceTransform::default(),
            workspace_natural_size: None,
            workspace_transforms: HashMap::new(),
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
      let (x, right) = window::scaled_edges(state.viewport.x, state.viewport.width, state.scale);
      let (y, bottom) = window::scaled_edges(state.viewport.y, state.viewport.height, state.scale);
      self
        .inner
        .editor
        .set_frame(x, y, right - x, bottom - y, state.editor_active);
      draw_selection(&self.inner, &state);
    }
  }

  pub(crate) fn enable_editor(&mut self, callback: TransformCallback) {
    if let Ok(mut callbacks) = self.inner.callbacks.lock() {
      callbacks.transform = Some(callback);
    }
    if let Ok(mut state) = self.inner.state.lock() {
      state.editor_active = true;
      state.frame_resize = None;
      state.frame_resize_committed = false;
      state.workspace_transform = WorkspaceTransform::default();
      state.workspace_natural_size = None;
      state.workspace_transforms.clear();
      let (x, right) = window::scaled_edges(state.viewport.x, state.viewport.width, state.scale);
      let (y, bottom) = window::scaled_edges(state.viewport.y, state.viewport.height, state.scale);
      self
        .inner
        .editor
        .set_frame(x, y, right - x, bottom - y, true);
      // The reset transform is 100%, so every pane returns to composing at
      // its on-screen size.
      draw_selection(&self.inner, &state);
    }
  }

  pub(crate) fn set_editor_active(&self, active: bool) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.editor_active = active;
      self.inner.editor.set_active(active);
      draw_selection(&self.inner, &state);
    }
  }

  pub(crate) fn set_editor_zoom(&self, zoom_percent: f64) {
    let zoom = (zoom_percent / 100.0).clamp(0.1, 16.0);
    let mut changed = false;
    if let Ok(mut state) = self.inner.state.lock() {
      if (state.workspace_transform.zoom - zoom).abs() > 0.0001 {
        let ratio = zoom / state.workspace_transform.zoom;
        state.workspace_transform.pan_x *= ratio;
        state.workspace_transform.pan_y *= ratio;
        state.workspace_transform.zoom = zoom;
        apply_workspace_transform(&self.inner, &mut state, false);
        changed = true;
      }
    }
    if changed {
      emit_transform(&self.inner, zoom);
    }
  }

  pub(crate) fn center_editor(&self) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.workspace_transform.pan_x = 0.0;
      state.workspace_transform.pan_y = 0.0;
      apply_workspace_transform(&self.inner, &mut state, false);
    }
  }

  pub(crate) fn set_selection_callback(&mut self, callback: SelectionCallback) {
    if let Ok(mut callbacks) = self.inner.callbacks.lock() {
      callbacks.selection = Some(callback);
    }
  }

  pub(crate) fn set_selection_gesture_callback(&mut self, callback: SelectionGestureCallback) {
    if let Ok(mut callbacks) = self.inner.callbacks.lock() {
      callbacks.gesture = Some(callback);
    }
  }

  pub(crate) fn set_selection_snapping(&self, enabled: bool) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.selection_snapping_enabled = enabled;
      if !enabled {
        clear_selection_snap_guides(&mut state);
        draw_selection(&self.inner, &state);
      }
    }
  }

  pub(crate) fn set_selection(&self, selection: Option<PreviewSelection>) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.selection = selection;
      if state.gesture.is_none() {
        clear_selection_snap_guides(&mut state);
      }
      draw_selection(&self.inner, &state);
    }
  }

  pub(crate) fn set_selection_visible(&self, visible: bool) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.selection_visible = visible;
      draw_selection(&self.inner, &state);
    }
  }

  pub(crate) fn set_selection_targets(&self, targets: Option<&[PreviewSelection]>) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.selection_targets.clear();
      state
        .selection_targets
        .extend_from_slice(targets.unwrap_or_default());
    }
  }

  pub(crate) fn set_viewport(&self, rect: PreviewSurfaceRect, backdrop: [f64; 4]) {
    if let Ok(mut state) = self.inner.state.lock() {
      state.viewport = rect;
      let (x, right) = window::scaled_edges(rect.x, rect.width, state.scale);
      let (y, bottom) = window::scaled_edges(rect.y, rect.height, state.scale);
      self
        .inner
        .editor
        .set_frame(x, y, right - x, bottom - y, state.editor_active);
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
      draw_selection(&self.inner, &state);
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
    self.layout_pane(&mut state, index, rect, defer_resize, false);
  }

  fn layout_pane(
    &self,
    state: &mut SurfaceState,
    index: u32,
    rect: PreviewSurfaceRect,
    defer_resize: bool,
    workspace: bool,
  ) {
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
    let transform = state.workspace_transform;
    // A live Frame resize owns the workspace geometry: the DOM rect still
    // describes the canvas the drag started from, so adopting it would
    // stretch the freshly composed canvas back into the old box until
    // mouse-up. The native re-flow already placed every pane.
    let owns_geometry = workspace && state.frame_resize.is_some();
    let Some(pane) = state.panes[index].as_mut() else {
      return;
    };
    pane.seen = true;
    if !owns_geometry {
      pane.base_rect = rect;
    }
    let transformed = transform.apply(viewport, pane.base_rect);
    // With a present on the way the geometry waits for it, so the pane's rect
    // and its freshly composed pixels land in the same commit. A pure pan (no
    // present coming) applies at once - but never while the DOM rect's aspect
    // has outrun the buffer: stretching the old frame into a new-aspect rect
    // for one transaction is exactly the jitter this avoids. Deferred
    // geometry is published by the pane's next present or the batch flush.
    set_pane_geometry(pane, viewport, transformed, scale, defer_resize);
  }

  /// Lays out the recording panes as one retained workspace. DirectComposition
  /// still uses one visual per source (each visual shares the same transform),
  /// but callers submit the complete pane topology in one operation and the
  /// existing batch keeps geometry and pixels atomic.
  pub(crate) fn layout_recording_workspace(
    &self,
    _rect: PreviewSurfaceRect,
    natural_size: (u32, u32),
    panes: &[(u32, PreviewSurfaceRect)],
    defer_draw: bool,
  ) {
    let mut restored_zoom = None;
    if let Ok(mut state) = self.inner.state.lock() {
      if state.frame_resize.is_some() {
        // A live drag owns the canvas size too: the re-flow tracks it from
        // the gesture's starts, and the DOM is a layout behind.
      } else if state.frame_resize_committed {
        // The drag rebased the transform so the displayed pixels stayed put;
        // its committed layout must keep that transform and record it as the
        // one belonging to the canvas size the drag produced, so a later undo
        // and redo across this size restore the same zoom.
        state.frame_resize_committed = false;
        state.workspace_natural_size = Some(natural_size);
        let transform = state.workspace_transform;
        state.workspace_transforms.insert(natural_size, transform);
      } else if state.workspace_natural_size != Some(natural_size) {
        if let Some(transform) = state.workspace_transforms.get(&natural_size).copied() {
          if (state.workspace_transform.zoom - transform.zoom).abs() > 0.0001 {
            restored_zoom = Some(transform.zoom);
          }
          state.workspace_transform.zoom = transform.zoom;
          state.workspace_transform.pan_x = 0.0;
          state.workspace_transform.pan_y = 0.0;
        }
        state.workspace_natural_size = Some(natural_size);
      }
      for (index, rect) in panes {
        self.layout_pane(&mut state, *index, *rect, defer_draw, true);
      }
    }
    if let Some(zoom) = restored_zoom {
      emit_transform(&self.inner, zoom);
    }
  }

  /// Applies one viewport-local transform to every pane while retaining their
  /// relative positions. Native Windows input will drive this directly.
  // Retained-workspace entry point; not wired on Windows yet.
  #[allow(dead_code)]
  pub(crate) fn set_workspace_transform(&self, pan_x: f64, pan_y: f64, zoom: f64) {
    let Ok(mut state) = self.inner.state.lock() else {
      return;
    };
    state.workspace_transform = WorkspaceTransform {
      pan_x,
      pan_y,
      zoom: zoom.clamp(0.1, 16.0),
    };
    apply_workspace_transform(&self.inner, &mut state, false);
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
      for pane in state.panes.iter_mut().flatten().filter(|pane| !pane.seen) {
        pane.hide();
        // A hidden pane has nothing stale to show; drop leftovers so a later
        // flush cannot resurrect it at a parked offset.
        pane.pending_geometry = false;
        pane.pending_present = false;
      }
      draw_selection(&self.inner, &state);
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
      None,
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
  // Retained-workspace entry point; not wired on Windows yet.
  #[allow(dead_code)]
  pub(crate) fn present_recording_workspace(
    &self,
    layers: &[RecordingWorkspaceLayer<'_>],
  ) -> Result<bool, String> {
    if layers.is_empty() {
      return Ok(false);
    }
    let batch = self.present_batch();
    let mut presented = false;
    for layer in layers {
      let Some(source) = layer.source else {
        // Keep this explicit rather than silently reading back a native
        // texture. The Windows decoder's D3D11 path will provide this branch
        // once its zero-copy source contract is promoted.
        if layer.source_pixels.is_some() {
          return Err("Windows recording workspace pixel sources are not wired".to_owned());
        }
        return Err("Recording workspace layer has no source".to_owned());
      };
      // The retained pane topology is already laid out by
      // `layout_recording_workspace`; the shared workspace transform is
      // applied by the pane visuals, so no CPU composition or intermediate
      // webview transport is introduced here.
      presented |= self.present_composed(
        layer.pane_index,
        layer.source_token,
        source,
        &layer.settings,
        layer.seconds,
        layer.cursor,
        layer.camera,
        layer.overlay,
        layer.clip_cursor_at_video_edge,
      )?;
    }
    drop(batch);
    Ok(presented)
  }

  #[allow(clippy::too_many_arguments)]
  // Retained-workspace entry point; not wired on Windows yet.
  #[allow(dead_code)]
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
      state.editor_active = false;
      state.selection = None;
      state.selection_targets.clear();
      state.gesture = None;
      // A hidden surface has no drag to finish; never keep the DOM locked out
      // of the pane geometry behind it.
      state.frame_resize = None;
      state.frame_resize_committed = false;
      self.inner.editor.set_active(false);
      draw_selection(&self.inner, &state);
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
      None,
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

#[cfg(test)]
mod tests {
  use super::*;

  fn rect(x: f64, y: f64, width: f64, height: f64) -> PreviewSurfaceRect {
    PreviewSurfaceRect {
      height,
      width,
      x,
      y,
    }
  }

  #[test]
  fn a_resized_pane_pushes_its_row_and_keeps_the_gaps() {
    let starts = vec![
      (0, rect(0.0, 0.0, 100.0, 100.0)),
      (1, rect(110.0, 0.0, 100.0, 50.0)),
      (2, rect(220.0, 0.0, 100.0, 100.0)),
    ];

    let reflowed = reflow_workspace_panes(&starts, 1, rect(110.0, -50.0, 200.0, 200.0));

    // The row keeps its 10pt gaps around the grown canvas, and every pane
    // stays centred on the row.
    assert_eq!(
      reflowed
        .iter()
        .map(|(index, rect)| (*index, rect.x, rect.y, rect.width, rect.height))
        .collect::<Vec<_>>(),
      vec![
        (0, 0.0, 0.0, 100.0, 100.0),
        (1, 110.0, -50.0, 200.0, 200.0),
        (2, 320.0, 0.0, 100.0, 100.0),
      ]
    );
  }

  #[test]
  fn a_mismatched_canvas_is_centred_in_its_box_and_a_matching_one_is_untouched() {
    let fitted = aspect_fit_rect(rect(10.0, 20.0, 200.0, 100.0), (100, 100));
    assert_eq!(
      (fitted.x, fitted.y, fitted.width, fitted.height),
      (60.0, 20.0, 100.0, 100.0)
    );

    let box_rect = rect(10.0, 20.0, 200.0, 100.0);
    let unchanged = aspect_fit_rect(box_rect, (1_920, 960));
    assert_eq!(
      (unchanged.x, unchanged.y, unchanged.width, unchanged.height),
      (box_rect.x, box_rect.y, box_rect.width, box_rect.height)
    );
  }
}
