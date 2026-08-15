// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Playback frame plumbing that is the same on every platform.
//!
//! The frame *contents* are platform-owned - see
//! [`super::platform::VideoFramePayload`] - because a backend that composites
//! on the GPU hands over decoded surfaces while a fallback backend hands over
//! encoded bytes for the webview to draw.

use super::platform::VideoFramePayload;

#[cfg_attr(target_os = "windows", allow(dead_code))]
pub(super) const PREVIEW_FPS: u64 = 30;

pub(super) struct VideoFrame {
  pub payload: VideoFramePayload,
  pub timestamp_ms: u64,
}
