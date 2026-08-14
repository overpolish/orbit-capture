// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform facade for preview decode, playback and thumbnails.
//!
//! [`super::super::preview_platform`] owns the *presentation* half of the
//! preview (the compositing surface below the webview) and carries the full
//! porting guide. This module owns the *production* half: getting pixels for a
//! given timestamp, at a given pane size, onto that surface.
//!
//! A backend must supply, with exactly these names and signatures:
//!
//! - [`VideoFramePayload`] - whatever a decoded playback frame is on this
//!   platform. A GPU backend carries decoded surfaces; a fallback backend
//!   carries encoded bytes the webview draws itself.
//! - [`send_frame`] - hand one payload to the surface (or to the webview).
//! - [`spawn_video`] - the playback decode thread.
//! - [`StillDecoder`] and [`NATIVE_STILLS`] - the paused-frame and scrub path.
//!   A backend without one sets `NATIVE_STILLS` to `false`, and the player
//!   routes stills through [`spawn_video`] with `still = true` instead. The
//!   [`StillDecoder`] type must still exist and compile; it is simply never
//!   constructed.
//! - [`playback_factors`] - how far each pane's decode shrinks toward its
//!   on-screen size.
//! - [`generate_thumbnails`] and [`source_frame_jpeg`] - the timeline strip and
//!   the one-off full-resolution frame the crop magnifier needs.
//!
//! Geometry, layout and settings math deliberately stay above this line, in
//! [`super::layout`] and the shared output validation, so a new backend never
//! reimplements them and never inherits Metal-specific assumptions.

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod fallback;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use fallback as backend;
#[cfg(target_os = "macos")]
use macos as backend;
#[cfg(target_os = "windows")]
use windows as backend;

#[cfg(target_os = "windows")]
pub(super) use backend::composed_frame_image;
#[cfg(target_os = "windows")]
pub(crate) use backend::GpuVideoReader;
pub(super) use backend::{
  generate_thumbnails, playback_factors, send_frame, source_frame_jpeg, spawn_video, StillDecoder,
  VideoFramePayload, NATIVE_STILLS,
};
