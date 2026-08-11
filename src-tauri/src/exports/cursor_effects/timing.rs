// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const MOTION_LOOKBACK_US: u64 = 800_000;
const MOTION_STEPS_PER_SECOND: f64 = 120.0;
const VELOCITY_TIME_CONSTANT: f64 = 0.03;
const ANGLE_TIME_CONSTANT: f64 = 0.045;
const MOTION_GRAVITY: f64 = 35.0;
const MAX_LEAN_DEGREES: f64 = 14.0;
const POSITION_SMOOTHING_RADIUS_US: u64 = 84_000;
const POSITION_SMOOTHING_SIGMA_US: f64 = 28_000.0;
const STOP_LOOKAHEAD_US: u64 = 100_000;
const STOP_SPEED: f64 = 0.35;

impl CursorCompositor {
  fn raw_position(&self, timestamp_us: u64) -> Option<Position> {
    let index = last_at_or_before(&self.positions, timestamp_us, |position| {
      position.timestamp_us
    })?;
    let current = self.positions[index];
    let Some(next) = self.positions.get(index + 1).copied() else {
      return Some(current);
    };
    let duration = next.timestamp_us.saturating_sub(current.timestamp_us);
    if duration == 0 || current.segment != next.segment {
      return Some(current);
    }
    let progress = timestamp_us.saturating_sub(current.timestamp_us) as f64 / duration as f64;
    Some(Position {
      segment: current.segment,
      timestamp_us,
      x: current.x + (next.x - current.x) * progress,
      y: current.y + (next.y - current.y) * progress,
    })
  }

  pub(super) fn smoothed_position(&self, timestamp_us: u64, enabled: bool) -> Option<Position> {
    let current = self.raw_position(timestamp_us)?;
    if !enabled {
      return Some(current);
    }
    // The complete recording lets us use a centred filter. Unlike a trailing
    // average, it removes capture jitter without making the rendered pointer
    // visibly chase the real one.
    let mut total_weight = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for index in -6_i64..=6 {
      let offset_us = index * POSITION_SMOOTHING_RADIUS_US as i64 / 6;
      let sample_us = timestamp_us.saturating_add_signed(offset_us);
      if let Some(sample) = self.raw_position(sample_us) {
        if sample.segment != current.segment {
          continue;
        }
        let weight = (-0.5 * (offset_us as f64 / POSITION_SMOOTHING_SIGMA_US).powi(2)).exp();
        x += sample.x * weight;
        y += sample.y * weight;
        total_weight += weight;
      }
    }
    if total_weight == 0.0 {
      return Some(current);
    }
    Some(Position {
      segment: current.segment,
      timestamp_us,
      x: x / total_weight,
      y: y / total_weight,
    })
  }

  pub(super) fn motion_lean_degrees(&self, timestamp_us: u64) -> f64 {
    let Some(current) = self.smoothed_position(timestamp_us, true) else {
      return 0.0;
    };
    let segment_start_us = self
      .positions
      .iter()
      .find(|position| position.segment == current.segment)
      .map_or(timestamp_us, |position| position.timestamp_us);
    let start_us = timestamp_us
      .saturating_sub(MOTION_LOOKBACK_US)
      .max(segment_start_us);
    let duration_seconds = timestamp_us.saturating_sub(start_us) as f64 / 1_000_000.0;
    let steps = (duration_seconds * MOTION_STEPS_PER_SECOND).ceil().max(1.0) as usize;
    let step_seconds = duration_seconds / steps as f64;
    if step_seconds <= 0.0 || self.source.width <= 0.0 {
      return 0.0;
    }
    let velocity_blend = 1.0 - (-step_seconds / VELOCITY_TIME_CONSTANT).exp();
    let angle_blend = 1.0 - (-step_seconds / ANGLE_TIME_CONSTANT).exp();
    let mut previous_x = self
      .smoothed_position(start_us, true)
      .filter(|position| position.segment == current.segment)
      .map_or(current.x / self.source.width, |position| {
        position.x / self.source.width
      });
    let mut smooth_velocity = 0.0;
    let mut previous_smooth_velocity = 0.0;
    let mut angle = 0.0;
    for step in 1..=steps {
      let sample_us = start_us
        .saturating_add((step_seconds * step as f64 * 1_000_000.0).round() as u64)
        .min(timestamp_us);
      let x = self
        .smoothed_position(sample_us, true)
        .filter(|position| position.segment == current.segment)
        .map_or(previous_x, |position| position.x / self.source.width);
      let velocity = (x - previous_x) / step_seconds;
      previous_x = x;
      smooth_velocity += (velocity - smooth_velocity) * velocity_blend;
      let acceleration = (smooth_velocity - previous_smooth_velocity) / step_seconds;
      previous_smooth_velocity = smooth_velocity;
      let target = (acceleration / MOTION_GRAVITY).atan().to_degrees();
      angle += (target - angle) * angle_blend;
    }
    angle.clamp(-MAX_LEAN_DEGREES, MAX_LEAN_DEGREES) * self.motion_envelope(timestamp_us)
  }

  fn motion_envelope(&self, timestamp_us: u64) -> f64 {
    if self.source.width <= 0.0 {
      return 0.0;
    }
    let Some(current) = self.raw_position(timestamp_us) else {
      return 0.0;
    };
    let Some(future) = self.raw_position(timestamp_us.saturating_add(STOP_LOOKAHEAD_US)) else {
      return 0.0;
    };
    if current.segment != future.segment {
      return 0.0;
    }
    let speed =
      ((future.x - current.x) / self.source.width).abs() / (STOP_LOOKAHEAD_US as f64 / 1_000_000.0);
    let progress = (speed / STOP_SPEED).clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
  }

  pub(super) fn click_scale(&self, timestamp_us: u64) -> f64 {
    let Some(index) = last_at_or_before(&self.button_events, timestamp_us, |event| {
      event.timestamp_us
    }) else {
      return 1.0;
    };
    let event = self.button_events[index];
    let elapsed_seconds = timestamp_us.saturating_sub(event.timestamp_us) as f64 / 1_000_000.0;
    match event.state {
      ButtonState::Down => pressed_scale(elapsed_seconds),
      ButtonState::Up => {
        let held_seconds = index
          .checked_sub(1)
          .and_then(|previous| self.button_events.get(previous))
          .filter(|previous| previous.state == ButtonState::Down)
          .map_or(f64::INFINITY, |down| {
            event.timestamp_us.saturating_sub(down.timestamp_us) as f64 / 1_000_000.0
          });
        released_scale(elapsed_seconds, pressed_scale(held_seconds))
      }
    }
  }
}

fn pressed_scale(elapsed_seconds: f64) -> f64 {
  let progress = 1.0 - (1.0 + 32.0 * elapsed_seconds) * (-32.0 * elapsed_seconds).exp();
  1.0 - 0.14 * progress.clamp(0.0, 1.0)
}

fn released_scale(elapsed_seconds: f64, released_from: f64) -> f64 {
  if elapsed_seconds >= 0.5 {
    return 1.0;
  }
  let damping = 0.68;
  let frequency = 22.0;
  let damped_frequency = frequency * f64::sqrt(1.0 - damping * damping);
  let response = 1.0
    - (-damping * frequency * elapsed_seconds).exp()
      * ((damped_frequency * elapsed_seconds).cos()
        + damping / f64::sqrt(1.0 - damping * damping)
          * (damped_frequency * elapsed_seconds).sin());
  released_from + (1.0 - released_from) * response
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::recording::cursor::CursorSourceKind;

  fn compositor(positions: Vec<Position>, button_events: Vec<ButtonEvent>) -> CursorCompositor {
    CursorCompositor {
      appearances: Vec::new(),
      button_events,
      positions,
      source: CursorSource {
        height: 1_000.0,
        kind: CursorSourceKind::Screen,
        platform_id: "test".to_owned(),
        video_height: 1_000,
        video_width: 1_000,
        width: 1_000.0,
        x: 0.0,
        y: 0.0,
      },
    }
  }

  #[test]
  fn click_holds_down_until_release_then_settles() {
    let compositor = compositor(
      Vec::new(),
      vec![
        ButtonEvent {
          state: ButtonState::Down,
          timestamp_us: 100_000,
        },
        ButtonEvent {
          state: ButtonState::Up,
          timestamp_us: 1_000_000,
        },
      ],
    );
    assert!((compositor.click_scale(500_000) - 0.86).abs() < 0.001);
    assert!((compositor.click_scale(1_000_000) - 0.86).abs() < 0.001);
    assert_ne!(compositor.click_scale(1_100_000), 1.0);
    assert_eq!(compositor.click_scale(1_500_000), 1.0);
  }

  #[test]
  fn motion_lean_is_visible_and_settles_after_motion() {
    let compositor = compositor(
      (0..=10)
        .map(|index| Position {
          segment: 0,
          timestamp_us: index * 50_000,
          x: index.min(8) as f64 * 100.0,
          y: 0.0,
        })
        .collect(),
      Vec::new(),
    );
    assert!(compositor.motion_lean_degrees(180_000) > 2.0);
    let braking_lean = compositor.motion_lean_degrees(350_000);
    assert!(
      braking_lean < 0.0,
      "lean should reverse before arrival, got {braking_lean}"
    );
    let arrival_lean = compositor.motion_lean_degrees(400_000);
    assert!(
      arrival_lean.abs() < 5.0,
      "lean should be nearly settled on arrival, got {arrival_lean}"
    );
    assert!(compositor.motion_lean_degrees(1_500_000).abs() < 0.1);
  }

  #[test]
  fn idle_gap_starts_a_fresh_motion_segment() {
    let compositor = compositor(
      vec![
        Position {
          segment: 0,
          timestamp_us: 0,
          x: 10.0,
          y: 10.0,
        },
        Position {
          segment: 0,
          timestamp_us: 50_000,
          x: 20.0,
          y: 10.0,
        },
        Position {
          segment: 1,
          timestamp_us: 500_000,
          x: 800.0,
          y: 600.0,
        },
      ],
      Vec::new(),
    );

    assert!(compositor.smoothed_position(450_000, true).unwrap().x < 30.0);
    let resumed = compositor.smoothed_position(500_000, true).unwrap();
    assert_eq!(resumed.segment, 1);
    assert!((resumed.x - 800.0).abs() < 0.001);
    assert_eq!(compositor.motion_lean_degrees(500_000), 0.0);
  }
}
