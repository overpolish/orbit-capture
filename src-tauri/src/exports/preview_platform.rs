// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform facade for the export window's live preview.
//!
//! # What this facade is
//!
//! The export window previews a recording or a screenshot by compositing frames
//! on the GPU into panes that sit *below* the OS webview. The webview draws only
//! UI chrome - on-screen controls, backdrop layers with CSS mask holes - so the
//! pixels the user is editing never cross IPC.
//!
//! Everything platform-specific about that lives behind this module and behind
//! [`super::recording_preview_player::platform`]. Shared Rust above the facade
//! owns geometry, layout and settings math (`recording_preview_player::layout`,
//! `media_preview::bake`, output validation) and must never assume a particular
//! GPU API.
//!
//! # Porting to a new platform
//!
//! A backend is a module that exports the same item names with the same
//! signatures as [`surface_macos`], selected by the `cfg` block below. Four
//! pieces make up a complete port; a partial port is legitimate, because
//! [`PreviewCapabilities`] lets a backend admit what it does not do yet and the
//! frontend falls back to the DOM/canvas preview for whatever is missing.
//!
//! 1. **A compositing surface created from a tauri window, rendering below the
//!    OS webview.** [`RecordingPreviewSurface::from_window`] takes the export
//!    [`WebviewWindow`] and attaches native panes as siblings *underneath* the
//!    webview's own view, so webview chrome composites on top of them. On
//!    macOS those are `CAMetalLayer`-backed views inserted below the
//!    `WKWebView` in the same `NSView` hierarchy (see
//!    `recording_preview_surface_macos.m`); on Windows the analogue is a
//!    DirectComposition visual tree, or a child HWND, ordered under the
//!    `WebView2` controller's HWND. The surface must provide batched pane
//!    layout ([`RecordingPreviewSurface::begin_layout`] / `layout` /
//!    `finish_layout`, so a resize is one atomic reposition and never tears
//!    against the webview), a viewport with an opaque backstop colour
//!    ([`RecordingPreviewSurface::set_viewport`] - the backstop is what shows
//!    through the webview's mask holes outside the panes),
//!    [`RecordingPreviewSurface::hide`], and the present-composed entry points
//!    below.
//! 2. **Present-composed-frame entry points.** `present` uploads a plain RGBA
//!    frame. [`RecordingPreviewSurface::present_composed`] takes a source image
//!    plus [`ScreenshotOutputSettings`] and does the whole output composition
//!    (background, rounding, shadow, cursor, camera overlay) on the GPU;
//!    `present_composed_pixels` is the zero-copy variant that takes an
//!    already-decoded platform pixel buffer so a playback frame never round
//!    trips through system memory. A backend that cannot do zero-copy yet may
//!    implement only `present_composed`.
//! 3. **A pane decoder and frame scrubber for stills and scrubbing**, plus (4)
//!    the **playback video path** - both live in
//!    [`super::recording_preview_player::platform`], which selects backends the
//!    same way this module does. The decoder answers "give me the frame at t,
//!    sized for this pane"; the scrubber keeps a warm long-lived decode context
//!    so dragging the timeline does not reopen a GOP per frame.
//! 5. **The export compositor** (writing the final file rather than the screen)
//!    is already split per platform in [`super::cursor_export`], under
//!    `platform_macos` / `platform_unsupported`.
//!
//! # Intended Windows/Linux GPU stack
//!
//! Windows should be implemented with **wgpu + WGSL**, not D3D directly. The
//! repo already carries wgpu for `crate::screenshots::mesh_gpu` with its
//! `mesh.wgsl` shader, so the composition shaders written for Windows can be
//! reused verbatim on Linux later. Consequently nothing in the shared layer may
//! encode Metal semantics: no `MTLPixelFormat`, no premultiplied-vs-straight
//! assumption beyond what [`crate::screenshots::CapturedImage`] documents, no
//! implicit top-left-origin texture convention. Keep those inside the backend.

use serde::Serialize;

#[cfg(target_os = "macos")]
#[path = "preview_platform/surface_macos.rs"]
mod surface;
#[cfg(target_os = "windows")]
#[path = "preview_platform/surface_windows.rs"]
mod surface;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[path = "preview_platform/surface_fallback.rs"]
mod surface;

#[cfg(target_os = "windows")]
pub(crate) use surface::ComposedFrame;
pub(crate) use surface::RecordingPreviewSurface;

/// Builds the comparatively expensive Windows D3D/DirectComposition pipeline
/// while the export webview is still hidden. The first recording review can
/// then open with only its media source to initialise.
#[cfg(target_os = "windows")]
pub(crate) fn prewarm(window: tauri::WebviewWindow) {
  tauri::async_runtime::spawn_blocking(move || {
    if let Err(error) = RecordingPreviewSurface::from_window(&window) {
      eprintln!("Could not prewarm the Windows preview surface: {error}");
    }
  });
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn prewarm(_window: tauri::WebviewWindow) {}

/// A pane or viewport rectangle in webview points, relative to the window.
///
/// Shared rather than per-backend: the webview reports the same geometry
/// whatever composites it.
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct PreviewSurfaceRect {
  pub height: f64,
  pub width: f64,
  pub x: f64,
  pub y: f64,
}

/// What the current platform's preview backend can actually do.
///
/// The frontend probes this instead of sniffing the platform, so a partially
/// implemented backend can enable one preview at a time: a Windows port that
/// has the still/screenshot path working but not video playback reports
/// `native_screenshot_preview: true` with `native_recording_preview: false`,
/// and only the recording preview keeps using the DOM/canvas fallback.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCapabilities {
  pub native_recording_preview: bool,
  pub native_screenshot_preview: bool,
}

/// Available on every platform; the fallback backend reports all-false.
#[tauri::command]
pub fn preview_capabilities() -> PreviewCapabilities {
  surface::CAPABILITIES
}
