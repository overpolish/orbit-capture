// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! Random-access frame decoding for paused frames and scrubbing, built on the
//! same `AVAssetReader` pipeline that playback uses.

use std::path::{Path, PathBuf};

use cidre::{arc, av};

use super::image::frame_position;
use super::scrubber::NativeFrameScrubber;
use super::scrubber::NativePixelFrame;
use super::video::{open_asset, NativeVideoReader};
use crate::screenshots::{CapturedImage, ScreenshotOutputSettings};

/// How far an exact seek rewinds before the target so the frame straddling a
/// seek position is still delivered.
const BACKWARD_TOLERANCE_MS: u64 = 100;
/// Decode width used before the webview has reported how large the preview
/// panes actually are on screen.
const FALLBACK_TARGET_WIDTH: u32 = 1_600;

/// One video track with a cached `AVAsset` and a resumable sequential reader.
pub(super) struct PaneDecoder {
  asset: arc::R<av::UrlAsset>,
  duration_ms: u64,
  path: PathBuf,
  reader: Option<ScrubReader>,
  scrubber: Option<ScrubberReader>,
  pub(super) source_height: u32,
  pub(super) source_width: u32,
}

pub(super) enum DecodedFrame {
  Pixels(NativePixelFrame),
  Rgba(CapturedImage),
}

impl DecodedFrame {
  pub(super) fn dimensions(&self) -> (u32, u32) {
    match self {
      Self::Pixels(frame) => (frame.width, frame.height),
      Self::Rgba(frame) => (frame.width, frame.height),
    }
  }

  pub(super) fn pixels(&self) -> Option<*mut std::ffi::c_void> {
    match self {
      Self::Pixels(frame) => Some(frame.as_ptr()),
      Self::Rgba(_) => None,
    }
  }

  pub(super) fn rgba(&self) -> Option<&CapturedImage> {
    match self {
      Self::Pixels(_) => None,
      Self::Rgba(frame) => Some(frame),
    }
  }

  pub(super) fn metadata(&self) -> CapturedImage {
    let (width, height) = self.dimensions();
    CapturedImage {
      height,
      rgba: Vec::new(),
      width,
    }
  }
}

struct ScrubReader {
  height: u32,
  last_target_ms: u64,
  reader: NativeVideoReader,
  width: u32,
}

struct ScrubberReader {
  height: u32,
  scrubber: NativeFrameScrubber,
  width: u32,
}

impl PaneDecoder {
  pub(super) fn open(
    path: &Path,
    source_width: u32,
    source_height: u32,
    duration_ms: u64,
  ) -> Result<Self, String> {
    let asset = open_asset(path)?;
    Ok(Self {
      asset,
      duration_ms,
      path: path.to_owned(),
      reader: None,
      scrubber: None,
      source_height,
      source_width,
    })
  }

  pub(super) fn decode_size(&self, factor: f64) -> (u32, u32) {
    let width = (f64::from(self.source_width) * factor).round().max(2.0) as u32;
    let height = (f64::from(self.source_height) * factor).round().max(2.0) as u32;
    (width.min(self.source_width), height.min(self.source_height))
  }

  fn reusable_forward(&self, target_ms: u64, width: u32, height: u32) -> bool {
    self.reader.as_ref().is_some_and(|reader| {
      reader.width == width
        && reader.height == height
        && target_ms >= reader.last_target_ms
        && target_ms - reader.last_target_ms < 2_000
    })
  }

  pub(super) fn frame_at(
    &mut self,
    position_ms: u64,
    width: u32,
    height: u32,
    rough: bool,
  ) -> Result<Option<DecodedFrame>, String> {
    let target_ms = frame_position(position_ms, self.duration_ms);
    let scrubber_matches = self
      .scrubber
      .as_ref()
      .is_some_and(|scrubber| scrubber.width == width && scrubber.height == height);
    if !scrubber_matches {
      // A pane changing on-screen size (zoom, bake toggle relayout) keeps its
      // warm player and only swaps the video output for the new dimensions.
      let resized = match self.scrubber.as_mut() {
        Some(existing) if existing.scrubber.resize(width, height) => {
          existing.width = width;
          existing.height = height;
          true
        }
        _ => false,
      };
      if !resized {
        self.scrubber = NativeFrameScrubber::open(&self.path, width, height)
          .ok()
          .map(|scrubber| ScrubberReader {
            height,
            scrubber,
            width,
          });
      }
    }
    if let Some(scrubber) = self.scrubber.as_ref() {
      if let Ok(frame) = scrubber.scrubber.frame_at(target_ms, rough) {
        return Ok(Some(DecodedFrame::Pixels(frame)));
      }
      self.scrubber = None;
    }
    // Keep the reader path as a defensive fallback for codecs that a player
    // output cannot vend. It is not the normal interactive scrub path.
    if !self.reusable_forward(target_ms, width, height) {
      let start_ms = target_ms.saturating_sub(BACKWARD_TOLERANCE_MS);
      let reset = match self.reader.as_mut() {
        Some(reader) if reader.width == width && reader.height == height => {
          let reset = reader.reader.reset(start_ms, self.duration_ms).is_ok();
          if reset {
            reader.last_target_ms = target_ms;
          }
          reset
        }
        _ => false,
      };
      if !reset {
        self.reader = Some(ScrubReader {
          height,
          last_target_ms: target_ms,
          reader: NativeVideoReader::open(&self.asset, width, height, start_ms, self.duration_ms)?,
          width,
        });
      }
    }
    let reader = self
      .reader
      .as_mut()
      .expect("a scrub reader exists after opening");
    reader.last_target_ms = target_ms;
    reader
      .reader
      .frame_at(target_ms)
      .map(|frame| frame.map(DecodedFrame::Rgba))
  }
}

pub(super) fn scaled_output(
  settings: &ScreenshotOutputSettings,
  factor: f64,
) -> ScreenshotOutputSettings {
  if factor >= 1.0 {
    return settings.clone();
  }
  // The compositor validates output dimensions at 64 pixels minimum, so a
  // tiny on-screen pane must not scale the composition below that.
  let minimum = (64.0 / f64::from(settings.width.max(1)))
    .max(64.0 / f64::from(settings.height.max(1)))
    .min(1.0);
  let factor = factor.max(minimum);
  let mut scaled = settings.clone();
  scaled.width = ((f64::from(settings.width) * factor).round().max(64.0)) as u32;
  scaled.height = ((f64::from(settings.height) * factor).round().max(64.0)) as u32;
  scaled
}

/// How much a pane's decode and composition shrink so the presented drawable
/// matches the on-screen pane instead of the full output resolution. Zooming
/// grows the reported target, so this reaches 1.0 exactly when the view needs
/// native pixels.
pub(super) fn pane_factor(target_sizes: &[(u32, u32)], index: usize, output_width: u32) -> f64 {
  if output_width == 0 {
    return 1.0;
  }
  // A pane that has not reported a real on-screen size yet (or was laid out
  // collapsed) must not shrink the composition to nothing.
  let target_width = target_sizes
    .get(index)
    .map(|size| size.0)
    .filter(|width| *width >= 16)
    .unwrap_or(FALLBACK_TARGET_WIDTH);
  (f64::from(target_width) / f64::from(output_width)).min(1.0)
}
