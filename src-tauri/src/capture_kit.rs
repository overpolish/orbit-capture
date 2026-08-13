// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

//! ScreenCaptureKit pieces shared by every macOS capture path.
//!
//! Stills and recordings resolve the same monitor, in the same units, and hide
//! the same windows. Keeping that in one place is what stops a recording from
//! quietly disagreeing with the screenshot taken a second earlier.

use std::collections::HashSet;

use cidre::{arc, ns, sc};

/// A monitor's scale and its size in physical pixels.
///
/// xcap reports macOS monitors in points, from `CGDisplayBounds`, whereas on
/// Windows it reports device pixels. Multiplying here is what lets both
/// platforms hand `physical_capture_rect` the same units.
pub fn monitor_geometry(monitor_id: u32) -> Result<(f64, u32, u32), String> {
  let monitor = xcap::Monitor::all()
    .map_err(|error| error.to_string())?
    .into_iter()
    .find(|monitor| monitor.id().ok() == Some(monitor_id))
    .ok_or_else(|| "The selected monitor is no longer available".to_owned())?;
  let scale = f64::from(monitor.scale_factor().map_err(|error| error.to_string())?);
  let width = f64::from(monitor.width().map_err(|error| error.to_string())?);
  let height = f64::from(monitor.height().map_err(|error| error.to_string())?);

  Ok((
    scale,
    (width * scale).round() as u32,
    (height * scale).round() as u32,
  ))
}

pub fn display_scale(display_id: u32) -> f64 {
  monitor_geometry(display_id).map_or(1.0, |(scale, _, _)| scale)
}

/// Excludes every window this process owns rather than a list of labels.
/// Matching on the owning process cannot drift: a window added later is
/// excluded the day it is added, with nothing to remember to update.
pub fn our_windows(content: &sc::ShareableContent) -> arc::R<ns::Array<sc::Window>> {
  let our_pid = std::process::id();
  let ours: Vec<_> = content
    .windows()
    .iter()
    .filter(|window| {
      window
        .owning_app()
        .is_some_and(|app| u32::try_from(app.process_id()).ok() == Some(our_pid))
    })
    .map(|window| window.retained())
    .collect();

  ns::Array::from_slice_retained(&ours)
}

/// The exclusion list a display filter should carry. Recording a demo of Orbit
/// Capture is the one case where its own windows belong in the shot, so the
/// list is simply empty and every window on the display is captured.
pub fn windows_to_exclude(
  content: &sc::ShareableContent,
  include_own_windows: bool,
) -> arc::R<ns::Array<sc::Window>> {
  if include_own_windows {
    ns::Array::new()
  } else {
    our_windows(content)
  }
}

/// Resolves the stable bundle identifiers stored by the UI against a fresh
/// ScreenCaptureKit snapshot. One application can own several processes, so
/// every matching entry is retained rather than stopping at the first.
pub fn application_audio_filter(
  content: &sc::ShareableContent,
  display: &sc::Display,
  application_ids: &[String],
) -> Result<arc::R<sc::ContentFilter>, String> {
  let selected = application_ids.iter().collect::<HashSet<_>>();
  let applications = content
    .apps()
    .iter()
    .filter(|application| selected.contains(&application.bundle_id().to_string()))
    .map(|application| application.retained())
    .collect::<Vec<_>>();
  if applications.is_empty() {
    return Err("None of the selected applications are currently available".into());
  }

  let applications = ns::Array::from_slice_retained(&applications);
  Ok(
    sc::ContentFilter::with_display_including_apps_excepting_windows(
      display,
      &applications,
      &ns::Array::new(),
    ),
  )
}
