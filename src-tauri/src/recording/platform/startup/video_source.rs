// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use crate::capture_geometry::{physical_capture_rect, video_capture_rect};
use crate::capture_kit::our_windows;
use crate::recording::Region;

pub(super) struct PrimaryVideo {
  pub filter: arc::R<sc::ContentFilter>,
  pub fps: u32,
  pub height: u32,
  pub is_window: bool,
  pub show_cursor: bool,
  pub source_rect: Option<cg::Rect>,
  pub source_scale_factor: f32,
  pub width: u32,
}

fn pixel_dimension(points: f64, scale: f32) -> Option<u32> {
  let pixels = points * f64::from(scale);
  if !pixels.is_finite() || pixels < 2.0 || pixels > f64::from(u32::MAX) {
    return None;
  }
  let pixels = pixels.round() as u32;
  let pixels = even(pixels);
  (pixels > 0).then_some(pixels)
}

fn display_video(
  content: &sc::ShareableContent,
  fps: u32,
  monitor_id: u32,
  region: Option<Region>,
  show_cursor: bool,
) -> Result<PrimaryVideo, String> {
  let displays = content.displays();
  let display = displays
    .iter()
    .find(|display| display.display_id().0 == monitor_id)
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
  let (scale, monitor_width, monitor_height) = monitor_geometry(monitor_id)?;
  let source_rect = region
    .map(|region| {
      physical_capture_rect(region, scale, monitor_width, monitor_height)
        .and_then(video_capture_rect)
        .ok_or_else(|| "The selected region is too small or outside the monitor".to_owned())
    })
    .transpose()?;
  let (width, height) = source_rect
    .map(|rect| (rect.width, rect.height))
    .unwrap_or_else(|| (even(monitor_width), even(monitor_height)));
  let source_rect = source_rect.map(|rect| {
    // ScreenCaptureKit's source rectangle is expressed in display points,
    // while the shared source contract is physical pixels.
    cg::Rect::new(
      f64::from(rect.x) / scale,
      f64::from(rect.y) / scale,
      f64::from(rect.width) / scale,
      f64::from(rect.height) / scale,
    )
  });

  Ok(PrimaryVideo {
    filter: sc::ContentFilter::with_display_excluding_windows(display, &our_windows(content)),
    fps,
    height,
    is_window: false,
    show_cursor,
    source_rect,
    source_scale_factor: scale as f32,
    width,
  })
}

fn window_video(
  content: &sc::ShareableContent,
  fps: u32,
  show_cursor: bool,
  window_id: u32,
) -> Result<PrimaryVideo, String> {
  let windows = content.windows();
  let window = windows
    .iter()
    .find(|window| window.id() == window_id)
    .ok_or_else(|| "The selected window is no longer available".to_owned())?;
  let filter = sc::ContentFilter::with_desktop_independent_window(window);
  let rect = filter.content_rect();
  let scale = filter.point_pixel_scale();
  let width = pixel_dimension(rect.size.width, scale)
    .ok_or_else(|| "The selected window has no usable width".to_owned())?;
  let height = pixel_dimension(rect.size.height, scale)
    .ok_or_else(|| "The selected window has no usable height".to_owned())?;

  Ok(PrimaryVideo {
    filter,
    fps,
    height,
    is_window: true,
    show_cursor,
    source_rect: None,
    source_scale_factor: scale,
    width,
  })
}

pub(super) fn resolve(
  content: &sc::ShareableContent,
  primary: &PrimaryCaptureSource,
) -> Result<Option<PrimaryVideo>, String> {
  match primary {
    PrimaryCaptureSource::Screen {
      fps,
      monitor_id,
      show_cursor,
    } => display_video(content, *fps, *monitor_id, None, *show_cursor).map(Some),
    PrimaryCaptureSource::Region {
      fps,
      monitor_id,
      region,
      show_cursor,
    } => display_video(content, *fps, *monitor_id, Some(*region), *show_cursor).map(Some),
    PrimaryCaptureSource::Window {
      fps,
      show_cursor,
      window_id,
    } => window_video(content, *fps, *show_cursor, *window_id).map(Some),
    PrimaryCaptureSource::Camera | PrimaryCaptureSource::Audio => Ok(None),
  }
}

#[cfg(test)]
mod tests {
  use super::pixel_dimension;

  #[test]
  fn trims_window_dimensions_to_encoder_safe_pixels() {
    assert_eq!(pixel_dimension(801.4, 2.0), Some(1_602));
    assert_eq!(pixel_dimension(800.6, 1.0), Some(800));
  }

  #[test]
  fn rejects_empty_or_invalid_window_dimensions() {
    assert_eq!(pixel_dimension(0.0, 2.0), None);
    assert_eq!(pixel_dimension(f64::NAN, 2.0), None);
    assert_eq!(pixel_dimension(100.0, 0.0), None);
  }
}
