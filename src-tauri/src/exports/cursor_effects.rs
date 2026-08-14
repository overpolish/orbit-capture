// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::recording::cursor::{
  self, ButtonState, CursorButton, CursorRecord, CursorSource, CursorStyle,
};
use std::path::Path;

mod overlay;
#[cfg(target_os = "macos")]
pub(in crate::exports) use overlay::CursorOverlayCache;
mod raster;
mod settings;
#[cfg(test)]
#[path = "cursor_effects/tests.rs"]
mod tests;
mod timing;
pub use settings::CursorEffectSettings;

const APPEARANCE_STABILITY_US: u64 = 300_000;
// ScreenCaptureKit's recorded cursor image trails the independently observed
// cursor position slightly, so macOS deliberately samples the sidecar earlier.
// Windows Graphics Capture and GetCursorInfo share the live screen timing; the
// macOS correction would make the baked Windows cursor visibly lag its native
// counterpart even when every cursor effect is disabled.
#[cfg(target_os = "macos")]
const SCREEN_REACTION_US: u64 = 2 * 1_000_000 / 60;
#[cfg(not(target_os = "macos"))]
const SCREEN_REACTION_US: u64 = 0;
const POSITION_SEGMENT_GAP_US: u64 = 100_000;
const POSITION_DWELL_US: u64 = 120_000;
const POSITION_DWELL_SPAN: f64 = 0.015;
const POSITION_DWELL_CORE: f64 = 0.4;
const MAX_BLUR_DISTANCE: f64 = 80.0;
const MAX_BLUR_SAMPLES: usize = 48;

#[derive(Clone, Copy)]
struct Appearance {
  height: f64,
  hotspot_x: f64,
  hotspot_y: f64,
  style: CursorStyle,
  timestamp_us: u64,
  width: f64,
}

#[derive(Clone, Copy)]
struct Position {
  segment: u32,
  timestamp_us: u64,
  x: f64,
  y: f64,
}

#[derive(Clone, Copy)]
struct ButtonEvent {
  state: ButtonState,
  timestamp_us: u64,
}

#[derive(Clone, Copy)]
struct DwellAnchor {
  end_us: u64,
  start_us: u64,
  x: f64,
  y: f64,
}

#[derive(Clone, Copy)]
struct EvaluatedCursor {
  appearance: Appearance,
  rotation_degrees: f64,
  scale: f64,
  segment: u32,
  x: f64,
  y: f64,
}

#[derive(Clone, Copy)]
struct OutputCursor {
  cursor: EvaluatedCursor,
  delta_x: f64,
  delta_y: f64,
  height: f64,
  hotspot_x: f64,
  hotspot_y: f64,
  width: f64,
  x: f64,
  y: f64,
}

fn output_hotspot(appearance: Appearance) -> (f64, f64) {
  #[cfg(target_os = "windows")]
  {
    if appearance.style == CursorStyle::Custom {
      // The sidecar does not carry custom cursor pixels yet, so custom
      // artwork falls back to the native arrow and must use its visible tip.
      (0.0, 0.0)
    } else {
      (appearance.hotspot_x, appearance.hotspot_y)
    }
  }
  #[cfg(not(target_os = "windows"))]
  {
    (appearance.hotspot_x, appearance.hotspot_y)
  }
}

/// Small, frame-local cursor description consumed by native GPU compositors.
/// Evaluating the event timeline is CPU work over a few numbers; cursor pixels,
/// animation, blur and blending remain entirely in the graphics shader.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuCursor {
  pub blur_delta_x: f32,
  pub blur_delta_y: f32,
  pub height: f32,
  pub hotspot_x: f32,
  pub hotspot_y: f32,
  pub rotation_radians: f32,
  pub scale: f32,
  pub style: u32,
  pub width: f32,
  pub x: f32,
  pub y: f32,
  pub clip_at_video_edge: bool,
}

#[derive(Clone, Copy)]
pub(in crate::exports) struct CursorOutputLayout {
  pub output_size: (u32, u32),
  pub image_rect: (f64, f64, f64, f64),
  pub clip_rect: Option<(i32, i32, u32, u32)>,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct CursorOverlayPosition {
  pub x: i32,
  pub y: i32,
}

#[derive(Clone)]
pub struct CursorCompositor {
  appearances: Vec<Appearance>,
  button_events: Vec<ButtonEvent>,
  dwell_anchors: Vec<DwellAnchor>,
  raw_positions: Vec<Position>,
  positions: Vec<Position>,
  source: CursorSource,
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
  let mut values = values.collect::<Vec<_>>();
  values.sort_by(f64::total_cmp);
  let middle = values.len() / 2;
  if values.len() % 2 == 0 {
    (values[middle - 1] + values[middle]) * 0.5
  } else {
    values[middle]
  }
}

/// Collapses a settled cloud of tiny OS cursor corrections into one held
/// position. Fast travel cannot qualify because the cloud must remain inside
/// the spatial radius for long enough to be a deliberate dwell.
fn stabilise_positions(positions: &[Position], source_width: f64) -> Vec<Position> {
  if positions.len() < 2 {
    return positions.to_vec();
  }
  let radius = source_width.max(1.0) * POSITION_DWELL_SPAN;
  let mut stable = Vec::with_capacity(positions.len());
  let mut start = 0;
  while start < positions.len() {
    let mut end = start + 1;
    let mut min_x = positions[start].x;
    let mut max_x = positions[start].x;
    let mut min_y = positions[start].y;
    let mut max_y = positions[start].y;
    while end < positions.len() {
      let next_min_x = min_x.min(positions[end].x);
      let next_max_x = max_x.max(positions[end].x);
      let next_min_y = min_y.min(positions[end].y);
      let next_max_y = max_y.max(positions[end].y);
      if (next_max_x - next_min_x).hypot(next_max_y - next_min_y) > radius {
        break;
      }
      min_x = next_min_x;
      max_x = next_max_x;
      min_y = next_min_y;
      max_y = next_max_y;
      end += 1;
    }
    let cloud = &positions[start..end];
    let x = median(cloud.iter().map(|position| position.x));
    let y = median(cloud.iter().map(|position| position.y));
    let core_radius = radius * POSITION_DWELL_CORE;
    let mut core_start = None;
    let mut core_end = None;
    for (offset, position) in cloud.iter().enumerate() {
      if (position.x - x).hypot(position.y - y) <= core_radius {
        core_start.get_or_insert(start + offset);
        core_end = Some(start + offset);
      }
    }
    if let (Some(core_start), Some(core_end)) = (core_start, core_end) {
      if positions[core_end]
        .timestamp_us
        .saturating_sub(positions[core_start].timestamp_us)
        < POSITION_DWELL_US
      {
        stable.push(positions[start]);
        start += 1;
        continue;
      }
      stable.extend_from_slice(&positions[start..core_start]);
      stable.push(Position {
        x,
        y,
        ..positions[core_start]
      });
      if core_end != core_start {
        stable.push(Position {
          x,
          y,
          ..positions[core_end]
        });
      }
      start = core_end + 1;
    } else {
      stable.push(positions[start]);
      start += 1;
    }
  }
  let mut segment = 0_u32;
  for index in 1..stable.len() {
    let previous = stable[index - 1];
    let current = &mut stable[index];
    let distance = (current.x - previous.x).hypot(current.y - previous.y);
    if current.timestamp_us.saturating_sub(previous.timestamp_us) > POSITION_SEGMENT_GAP_US
      && distance > radius
    {
      segment = segment.saturating_add(1);
    }
    current.segment = segment;
  }
  stable
}

fn segment_raw_positions(positions: &mut [Position]) {
  let mut segment = 0_u32;
  for index in 1..positions.len() {
    if positions[index]
      .timestamp_us
      .saturating_sub(positions[index - 1].timestamp_us)
      > POSITION_SEGMENT_GAP_US
    {
      segment = segment.saturating_add(1);
    }
    positions[index].segment = segment;
  }
}

fn dwell_anchors(positions: &[Position]) -> Vec<DwellAnchor> {
  positions
    .windows(2)
    .filter_map(|pair| {
      let current = pair[0];
      let next = pair[1];
      let held_us = next.timestamp_us.saturating_sub(current.timestamp_us);
      (held_us >= POSITION_DWELL_US).then_some(DwellAnchor {
        end_us: next.timestamp_us,
        start_us: current.timestamp_us,
        x: current.x,
        y: current.y,
      })
    })
    .collect()
}

fn last_at_or_before<T>(
  values: &[T],
  timestamp_us: u64,
  timestamp: impl Fn(&T) -> u64,
) -> Option<usize> {
  let index = values.partition_point(|value| timestamp(value) <= timestamp_us);
  index.checked_sub(1)
}

fn stable_appearances(appearances: &[Appearance], recording_end_us: u64) -> Vec<Appearance> {
  let mut changes = Vec::new();
  for appearance in appearances {
    if changes.last().is_some_and(|previous: &Appearance| {
      raster::uses_same_artwork(previous.style, appearance.style)
    }) {
      continue;
    }
    changes.push(*appearance);
  }
  let Some(first) = changes.first().copied() else {
    return Vec::new();
  };
  let mut stable = vec![first];
  for (index, appearance) in changes.iter().enumerate().skip(1) {
    let end_us = changes
      .get(index + 1)
      .map_or(recording_end_us, |next| next.timestamp_us);
    if end_us.saturating_sub(appearance.timestamp_us) < APPEARANCE_STABILITY_US {
      continue;
    }
    if stable
      .last()
      .is_none_or(|previous| !raster::uses_same_artwork(previous.style, appearance.style))
    {
      stable.push(*appearance);
    }
  }
  stable
}

fn motion_blur_sample_count(distance: f64) -> usize {
  ((distance / 2.0).ceil() as usize + 1).clamp(8, MAX_BLUR_SAMPLES)
}

impl CursorCompositor {
  pub fn open(path: &Path) -> Result<Self, String> {
    let records = cursor::read(path)?;
    Self::from_records(&records)
  }

  fn from_records(records: &[CursorRecord]) -> Result<Self, String> {
    let source = records
      .iter()
      .find_map(|record| match record {
        CursorRecord::Header { source, .. } => Some(source.clone()),
        _ => None,
      })
      .ok_or_else(|| "The cursor recording has no source".to_owned())?;
    let mut appearances: Vec<_> = records
      .iter()
      .filter_map(|record| match record {
        CursorRecord::Appearance {
          height,
          hotspot_x,
          hotspot_y,
          style,
          timestamp_us,
          width,
        } => Some(Appearance {
          height: *height,
          hotspot_x: *hotspot_x,
          hotspot_y: *hotspot_y,
          style: *style,
          timestamp_us: *timestamp_us,
          width: *width,
        }),
        _ => None,
      })
      .collect();
    appearances.sort_by_key(|appearance| appearance.timestamp_us);
    let mut raw_positions: Vec<_> = records
      .iter()
      .filter_map(|record| match record {
        CursorRecord::Position { timestamp_us, x, y }
        | CursorRecord::Button {
          timestamp_us, x, y, ..
        } => Some(Position {
          segment: 0,
          timestamp_us: *timestamp_us,
          x: *x,
          y: *y,
        }),
        _ => None,
      })
      .collect();
    raw_positions.sort_by_key(|position| position.timestamp_us);
    // macOS's cursor smoothing is already tuned and shipped. Windows polling
    // exposes explicit held intervals that must remain exact anchors.
    let dwell_anchors = if cfg!(target_os = "windows") {
      dwell_anchors(&raw_positions)
    } else {
      Vec::new()
    };
    segment_raw_positions(&mut raw_positions);
    let positions = if cfg!(target_os = "windows") {
      stabilise_positions(&raw_positions, source.width)
    } else {
      // Event-driven macOS cursor positions are already the canonical path.
      // Keep its proven smoothing input free of Windows polling heuristics.
      raw_positions.clone()
    };
    let recording_end_us = raw_positions
      .last()
      .map_or(0, |position| position.timestamp_us);
    let stable = stable_appearances(&appearances, recording_end_us);
    let mut raw_button_events = records
      .iter()
      .filter_map(|record| match record {
        CursorRecord::Button {
          button,
          state,
          timestamp_us,
          ..
        } => Some((*timestamp_us, *button, *state)),
        _ => None,
      })
      .collect::<Vec<_>>();
    raw_button_events.sort_by_key(|(timestamp_us, ..)| *timestamp_us);
    let mut pressed = Vec::<CursorButton>::new();
    let mut button_events = Vec::new();
    for (timestamp_us, button, state) in raw_button_events {
      match state {
        ButtonState::Down if !pressed.contains(&button) => {
          if pressed.is_empty() {
            button_events.push(ButtonEvent {
              state,
              timestamp_us,
            });
          }
          pressed.push(button);
        }
        ButtonState::Up if pressed.contains(&button) => {
          pressed.retain(|pressed_button| *pressed_button != button);
          if pressed.is_empty() {
            button_events.push(ButtonEvent {
              state,
              timestamp_us,
            });
          }
        }
        _ => {}
      }
    }
    Ok(Self {
      appearances: stable,
      button_events,
      dwell_anchors,
      raw_positions,
      positions,
      source,
    })
  }

  fn output_cursor(
    &self,
    position_ms: u64,
    width: usize,
    height: usize,
    settings: CursorEffectSettings,
  ) -> Option<OutputCursor> {
    let timestamp_us = position_ms
      .saturating_mul(1_000)
      .saturating_sub(SCREEN_REACTION_US);
    let cursor = self.evaluate(timestamp_us, settings)?;
    let (hotspot_x, hotspot_y) = output_hotspot(cursor.appearance);
    let previous = self.evaluate(timestamp_us.saturating_sub(1_000_000 / 60), settings);
    let (delta_x, delta_y) = previous
      .filter(|previous| previous.segment == cursor.segment)
      .map_or((0.0, 0.0), |previous| {
        (
          (cursor.x - previous.x) / self.source.width * width as f64,
          (cursor.y - previous.y) / self.source.height * height as f64,
        )
      });
    Some(OutputCursor {
      cursor,
      delta_x,
      delta_y,
      height: cursor.appearance.height / self.source.height * height as f64,
      hotspot_x: hotspot_x / self.source.width * width as f64,
      hotspot_y: hotspot_y / self.source.height * height as f64,
      width: cursor.appearance.width / self.source.width * width as f64,
      x: (cursor.x - self.source.x) / self.source.width * width as f64,
      y: (cursor.y - self.source.y) / self.source.height * height as f64,
    })
  }

  pub(in crate::exports) fn gpu_cursor(
    &self,
    position_ms: u64,
    source_size: (u32, u32),
    settings: CursorEffectSettings,
  ) -> Option<GpuCursor> {
    let output = self.output_cursor(
      position_ms,
      source_size.0 as usize,
      source_size.1 as usize,
      settings,
    )?;
    let artwork = match output.cursor.appearance.style {
      CursorStyle::IBeam => 1,
      CursorStyle::VerticalIBeam => 2,
      CursorStyle::ResizeHorizontal => 3,
      CursorStyle::ResizeVertical => 4,
      CursorStyle::PointingHand | CursorStyle::ClosedHand | CursorStyle::OpenHand => 5,
      CursorStyle::Crosshair => 6,
      CursorStyle::NotAllowed => 7,
      _ => 0,
    };
    Some(GpuCursor {
      blur_delta_x: if settings.motion_blur {
        output.delta_x as f32
      } else {
        0.0
      },
      blur_delta_y: if settings.motion_blur {
        output.delta_y as f32
      } else {
        0.0
      },
      height: output.height as f32,
      hotspot_x: output.hotspot_x as f32,
      hotspot_y: output.hotspot_y as f32,
      rotation_radians: output.cursor.rotation_degrees.to_radians() as f32,
      scale: (output.cursor.scale * settings.size_percent.clamp(50.0, 500.0) / 100.0) as f32,
      style: artwork,
      width: output.width as f32,
      x: output.x as f32,
      y: output.y as f32,
      clip_at_video_edge: settings.clip_at_video_edge,
    })
  }

  /// Composes one explicit still-frame request. Live Windows preview never
  /// calls this path; it exists for clipboard/export operations that need an
  /// owned bitmap.
  pub(in crate::exports) fn composite_output_rgba(
    &self,
    pixels: &mut [u8],
    source_size: (u32, u32),
    position_ms: u64,
    settings: CursorEffectSettings,
    layout: CursorOutputLayout,
  ) -> Result<(), String> {
    let CursorOutputLayout {
      output_size,
      image_rect,
      clip_rect,
    } = layout;
    let expected = output_size.0 as usize * output_size.1 as usize * 4;
    if pixels.len() != expected {
      return Err("The clipboard frame pixels are invalid".to_owned());
    }
    let Some(mut output) = self.output_cursor(
      position_ms,
      source_size.0 as usize,
      source_size.1 as usize,
      settings,
    ) else {
      return Ok(());
    };
    let scale_x = image_rect.2 / f64::from(source_size.0);
    let scale_y = image_rect.3 / f64::from(source_size.1);
    output.x = image_rect.0 + output.x * scale_x;
    output.y = image_rect.1 + output.y * scale_y;
    output.width *= scale_x;
    output.height *= scale_y;
    output.hotspot_x *= scale_x;
    output.hotspot_y *= scale_y;
    output.delta_x *= scale_x;
    output.delta_y *= scale_y;
    let before = clip_rect.map(|_| pixels.to_vec());
    for pixel in pixels.chunks_exact_mut(4) {
      pixel.swap(0, 2);
    }
    self.draw_output(
      &mut raster::FrameMut {
        height: output_size.1 as usize,
        pixels,
        stride: output_size.0 as usize * 4,
        width: output_size.0 as usize,
      },
      output,
      output.x,
      output.y,
      settings,
    );
    for pixel in pixels.chunks_exact_mut(4) {
      pixel.swap(0, 2);
    }
    if let (Some(before), Some((clip_x, clip_y, clip_width, clip_height))) = (before, clip_rect) {
      let right = i64::from(clip_x) + i64::from(clip_width);
      let bottom = i64::from(clip_y) + i64::from(clip_height);
      for y in 0..output_size.1 {
        for x in 0..output_size.0 {
          if i64::from(x) < i64::from(clip_x)
            || i64::from(x) >= right
            || i64::from(y) < i64::from(clip_y)
            || i64::from(y) >= bottom
          {
            let offset = ((y * output_size.0 + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&before[offset..offset + 4]);
          }
        }
      }
    }
    Ok(())
  }

  fn draw_output(
    &self,
    frame: &mut raster::FrameMut<'_>,
    output: OutputCursor,
    x: f64,
    y: f64,
    settings: CursorEffectSettings,
  ) {
    let scale = output.cursor.scale * settings.size_percent.clamp(50.0, 500.0) / 100.0;
    let travel = output.delta_x.hypot(output.delta_y);
    let blur_distance = if settings.motion_blur {
      travel.min(MAX_BLUR_DISTANCE)
    } else {
      0.0
    };
    let raster = raster::CursorRaster::new(
      output.cursor.appearance.style,
      output.cursor.rotation_degrees,
      output.width,
      output.height,
      output.hotspot_x,
      output.hotspot_y,
      scale,
    );
    if blur_distance > 1.25 && travel > 0.0 {
      // Keep exposure samples no more than two output pixels apart. Scaling
      // this by cursor size left distinct copies visible on large pointers.
      let sample_count = motion_blur_sample_count(blur_distance);
      raster::draw_blurred(
        frame,
        raster,
        x,
        y,
        output.delta_x / travel,
        output.delta_y / travel,
        blur_distance,
        sample_count,
      );
      return;
    }
    raster::draw(frame, raster, x, y);
  }

  fn evaluate(&self, timestamp_us: u64, settings: CursorEffectSettings) -> Option<EvaluatedCursor> {
    let appearance = *self.appearances.get(last_at_or_before(
      &self.appearances,
      timestamp_us,
      |appearance| appearance.timestamp_us,
    )?)?;
    let current = self.smoothed_position(timestamp_us, settings.smooth_movement)?;
    let rotation_degrees = if settings.smooth_movement {
      let cursor_size_pixels =
        appearance.width.max(appearance.height) * settings.size_percent.clamp(50.0, 500.0) / 100.0;
      self.motion_lean_degrees(timestamp_us, cursor_size_pixels)
    } else {
      0.0
    };
    Some(EvaluatedCursor {
      appearance,
      rotation_degrees,
      scale: if settings.click_animation {
        self.click_scale(timestamp_us)
      } else {
        1.0
      },
      segment: current.segment,
      x: current.x,
      y: current.y,
    })
  }
}

pub(crate) fn initialize_artwork() {
  raster::initialize_system_artwork();
}
