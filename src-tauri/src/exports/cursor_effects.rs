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
const SCREEN_REACTION_US: u64 = 2 * 1_000_000 / 60;
const POSITION_SEGMENT_GAP_US: u64 = 100_000;
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

#[derive(Clone, Copy, PartialEq)]
pub(super) struct CursorOverlayPosition {
  pub x: i32,
  pub y: i32,
}

#[derive(Clone)]
pub struct CursorCompositor {
  appearances: Vec<Appearance>,
  button_events: Vec<ButtonEvent>,
  positions: Vec<Position>,
  source: CursorSource,
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
    let mut positions: Vec<_> = records
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
    positions.sort_by_key(|position| position.timestamp_us);
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
    let recording_end_us = positions.last().map_or(0, |position| position.timestamp_us);
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
      hotspot_x: cursor.appearance.hotspot_x / self.source.width * width as f64,
      hotspot_y: cursor.appearance.hotspot_y / self.source.height * height as f64,
      width: cursor.appearance.width / self.source.width * width as f64,
      x: (cursor.x - self.source.x) / self.source.width * width as f64,
      y: (cursor.y - self.source.y) / self.source.height * height as f64,
    })
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
      self.motion_lean_degrees(timestamp_us)
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
