// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const MOTION_LOOKBACK_US: u64 = 140_000;
const MOTION_LOOKAHEAD_US: u64 = 220_000;
const MOTION_SAMPLES: usize = 24;
const MOTION_WARMUP_US: u64 = 140_000;
const MAX_LEAN_DEGREES: f64 = 14.0;
const FULL_LEAN_DISTANCE: f64 = 0.09;
const FULL_LEAN_SPEED: f64 = 0.65;
const MIN_LEAN_DISTANCE: f64 = 0.006;
const MIN_LEAN_SPEED: f64 = 0.03;
const POSITION_SMOOTHING_RADIUS_US: u64 = 84_000;
const POSITION_SMOOTHING_SIGMA_US: f64 = 28_000.0;
const POSITION_ANCHOR_EASING_US: u64 = 84_000;
const REFERENCE_CURSOR_SIZE: f64 = 32.0;

impl CursorCompositor {
  fn position_at(&self, timestamp_us: u64, stabilised: bool) -> Option<Position> {
    let positions = if stabilised {
      &self.positions
    } else {
      &self.raw_positions
    };
    let index = last_at_or_before(positions, timestamp_us, |position| position.timestamp_us)?;
    let current = positions[index];
    let Some(next) = positions.get(index + 1).copied() else {
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
    let current = self.position_at(timestamp_us, enabled)?;
    if !enabled {
      return Some(current);
    }
    // The complete recording lets us use a centred filter. Unlike a trailing
    // average, it removes capture jitter without making the rendered pointer
    // visibly chase the real one.
    let filtered = self.gaussian_position(
      timestamp_us,
      current,
      POSITION_SMOOTHING_RADIUS_US,
      POSITION_SMOOTHING_SIGMA_US,
    );
    Some(self.anchored_position(timestamp_us, filtered))
  }

  fn anchored_position(&self, timestamp_us: u64, filtered: Position) -> Position {
    let next_index = self
      .dwell_anchors
      .partition_point(|anchor| anchor.start_us <= timestamp_us);
    if let Some(anchor) = next_index
      .checked_sub(1)
      .and_then(|index| self.dwell_anchors.get(index))
    {
      if timestamp_us <= anchor.end_us {
        return Position {
          x: anchor.x,
          y: anchor.y,
          ..filtered
        };
      }
      let elapsed = timestamp_us.saturating_sub(anchor.end_us);
      if elapsed < POSITION_ANCHOR_EASING_US {
        let progress = smoothstep(0.0, POSITION_ANCHOR_EASING_US as f64, elapsed as f64);
        return Position {
          x: anchor.x + (filtered.x - anchor.x) * progress,
          y: anchor.y + (filtered.y - anchor.y) * progress,
          ..filtered
        };
      }
    }
    if let Some(anchor) = self.dwell_anchors.get(next_index) {
      let remaining = anchor.start_us.saturating_sub(timestamp_us);
      if remaining < POSITION_ANCHOR_EASING_US {
        let progress = smoothstep(
          0.0,
          POSITION_ANCHOR_EASING_US as f64,
          POSITION_ANCHOR_EASING_US.saturating_sub(remaining) as f64,
        );
        return Position {
          x: filtered.x + (anchor.x - filtered.x) * progress,
          y: filtered.y + (anchor.y - filtered.y) * progress,
          ..filtered
        };
      }
    }
    filtered
  }

  fn gaussian_position(
    &self,
    timestamp_us: u64,
    current: Position,
    radius_us: u64,
    sigma_us: f64,
  ) -> Position {
    let mut total_weight = 0.0;
    let mut x = 0.0;
    let mut y = 0.0;
    for index in -6_i64..=6 {
      let offset_us = index * radius_us as i64 / 6;
      let sample_us = timestamp_us.saturating_add_signed(offset_us);
      if let Some(sample) = self.position_at(sample_us, true) {
        if sample.segment != current.segment {
          continue;
        }
        let weight = (-0.5 * (offset_us as f64 / sigma_us).powi(2)).exp();
        x += sample.x * weight;
        y += sample.y * weight;
        total_weight += weight;
      }
    }
    if total_weight == 0.0 {
      return current;
    }
    Position {
      segment: current.segment,
      timestamp_us,
      x: x / total_weight,
      y: y / total_weight,
    }
  }

  pub(super) fn motion_lean_degrees(&self, timestamp_us: u64, cursor_size_pixels: f64) -> f64 {
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
    let end_us = timestamp_us.saturating_add(MOTION_LOOKAHEAD_US);
    let duration_seconds = end_us.saturating_sub(start_us) as f64 / 1_000_000.0;
    if duration_seconds <= 0.0 || self.source.width <= 0.0 {
      return 0.0;
    }

    // Rotation represents momentum along the visible trajectory, not the
    // instantaneous acceleration between two cursor samples. Looking ahead
    // carries a fast gesture naturally into a connected slower section, while
    // the accumulated path distance prevents tiny fast flicks from leaning as
    // much as a deliberate long movement.
    let mut first: Option<Position> = None;
    let mut previous: Option<Position> = None;
    let mut last: Option<Position> = None;
    let mut path_distance = 0.0;
    for step in 0..=MOTION_SAMPLES {
      let sample_us = start_us.saturating_add(
        (end_us.saturating_sub(start_us) as u128 * step as u128 / MOTION_SAMPLES as u128) as u64,
      );
      let Some(sample) = self
        .smoothed_position(sample_us, true)
        .filter(|position| position.segment == current.segment)
      else {
        continue;
      };
      if let Some(previous_position) = previous {
        let delta_x: f64 = sample.x - previous_position.x;
        let delta_y: f64 = sample.y - previous_position.y;
        path_distance += delta_x.hypot(delta_y);
      } else {
        first = Some(sample);
      }
      previous = Some(sample);
      last = Some(sample);
    }
    let (Some(first), Some(last)) = (first, last) else {
      return 0.0;
    };
    if path_distance <= f64::EPSILON {
      return 0.0;
    }
    let normalized_distance = path_distance / self.source.width;
    let normalized_speed = normalized_distance / duration_seconds;
    let distance_weight = smoothstep(MIN_LEAN_DISTANCE, FULL_LEAN_DISTANCE, normalized_distance);
    let speed_weight = smoothstep(MIN_LEAN_SPEED, FULL_LEAN_SPEED, normalized_speed);
    let horizontal_direction = ((last.x - first.x) / path_distance).clamp(-1.0, 1.0);
    // Larger artwork needs more visual inertia. Square-root scaling keeps a
    // giant cursor restrained without making its motion completely rigid.
    let size_weight = (REFERENCE_CURSOR_SIZE / cursor_size_pixels.max(1.0))
      .sqrt()
      .clamp(0.4, 1.4);
    let warmup = smoothstep(
      0.0,
      MOTION_WARMUP_US as f64,
      timestamp_us.saturating_sub(segment_start_us) as f64,
    );
    MAX_LEAN_DEGREES * horizontal_direction * distance_weight * speed_weight * size_weight * warmup
  }

  pub(super) fn click_scale(&self, timestamp_us: u64) -> f64 {
    let Some(index) = last_at_or_before(&self.button_events, timestamp_us, |event| {
      event.timestamp_us
    }) else {
      return 1.0;
    };
    self.click_scale_after(index, timestamp_us)
  }

  fn click_scale_after(&self, index: usize, timestamp_us: u64) -> f64 {
    let event = self.button_events[index];
    let elapsed_seconds = timestamp_us.saturating_sub(event.timestamp_us) as f64 / 1_000_000.0;
    // Once a release has settled it is also a natural history boundary. This
    // keeps evaluation constant-size for long recordings with many clicks.
    if event.state == ButtonState::Up && elapsed_seconds >= 0.5 {
      return 1.0;
    }
    let start = index.checked_sub(1).map_or(1.0, |previous| {
      self.click_scale_after(previous, event.timestamp_us)
    });
    match event.state {
      ButtonState::Down => pressed_scale(elapsed_seconds, start),
      ButtonState::Up => released_scale(elapsed_seconds, start),
    }
  }
}

fn smoothstep(start: f64, end: f64, value: f64) -> f64 {
  let progress = ((value - start) / (end - start)).clamp(0.0, 1.0);
  progress * progress * (3.0 - 2.0 * progress)
}

fn pressed_scale(elapsed_seconds: f64, start: f64) -> f64 {
  let progress = 1.0 - (1.0 + 32.0 * elapsed_seconds) * (-32.0 * elapsed_seconds).exp();
  start + (0.86 - start) * progress.clamp(0.0, 1.0)
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
    let raw_positions = positions.clone();
    CursorCompositor {
      appearances: Vec::new(),
      button_events,
      dwell_anchors: Vec::new(),
      raw_positions,
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
  fn repeated_click_does_not_jump_back_to_full_size() {
    let compositor = compositor(
      Vec::new(),
      vec![
        ButtonEvent {
          state: ButtonState::Down,
          timestamp_us: 0,
        },
        ButtonEvent {
          state: ButtonState::Up,
          timestamp_us: 100_000,
        },
        ButtonEvent {
          state: ButtonState::Down,
          timestamp_us: 200_000,
        },
      ],
    );
    let before = compositor.click_scale(199_999);
    let after = compositor.click_scale(200_000);
    assert!((before - after).abs() < 0.001);
  }

  #[test]
  fn motion_lean_carries_through_connected_fast_and_slow_motion() {
    let compositor = compositor(
      [
        (0, 0.0),
        (50_000, 150.0),
        (100_000, 300.0),
        (150_000, 450.0),
        (200_000, 500.0),
        (300_000, 550.0),
        (400_000, 600.0),
        (500_000, 650.0),
      ]
      .into_iter()
      .map(|(timestamp_us, x)| Position {
        segment: 0,
        timestamp_us,
        x,
        y: 0.0,
      })
      .collect(),
      Vec::new(),
    );
    let fast_lean = compositor.motion_lean_degrees(100_000, 32.0);
    let transition_lean = compositor.motion_lean_degrees(190_000, 32.0);
    let slow_lean = compositor.motion_lean_degrees(320_000, 32.0);
    assert!(fast_lean > 2.0);
    assert!(
      transition_lean > fast_lean * 0.6,
      "momentum collapsed at the speed transition: {transition_lean}"
    );
    assert!(
      slow_lean > 0.5,
      "the connected slow movement lost all momentum: {slow_lean}"
    );
    assert!(compositor.motion_lean_degrees(1_500_000, 32.0).abs() < 0.1);
  }

  #[test]
  fn motion_lean_eases_in_before_reaching_its_full_angle() {
    let compositor = compositor(
      vec![
        Position {
          segment: 0,
          timestamp_us: 0,
          x: 0.0,
          y: 0.0,
        },
        Position {
          segment: 0,
          timestamp_us: 250_000,
          x: 800.0,
          y: 0.0,
        },
      ],
      Vec::new(),
    );
    let initial = compositor.motion_lean_degrees(0, 32.0);
    let entering = compositor.motion_lean_degrees(35_000, 32.0);
    let established = compositor.motion_lean_degrees(160_000, 32.0);
    assert_eq!(initial, 0.0);
    assert!(entering > 0.0);
    assert!(
      entering < established * 0.3,
      "lean skipped its warmup: entering={entering}, established={established}"
    );
  }

  #[test]
  fn short_fast_flick_rotates_less_than_a_long_fast_move() {
    let movement = |distance| {
      compositor(
        vec![
          Position {
            segment: 0,
            timestamp_us: 0,
            x: 0.0,
            y: 0.0,
          },
          Position {
            segment: 0,
            timestamp_us: 20_000,
            x: distance,
            y: 0.0,
          },
        ],
        Vec::new(),
      )
    };
    let short = movement(20.0).motion_lean_degrees(10_000, 32.0);
    let long = movement(300.0).motion_lean_degrees(10_000, 32.0);
    assert!(short < 2.0, "short flick leaned too far: {short}");
    assert!(
      long > short * 3.0,
      "distance did not shape lean: {short} vs {long}"
    );
  }

  #[test]
  fn giant_cursor_has_more_rotational_inertia() {
    let compositor = compositor(
      vec![
        Position {
          segment: 0,
          timestamp_us: 0,
          x: 0.0,
          y: 0.0,
        },
        Position {
          segment: 0,
          timestamp_us: 150_000,
          x: 500.0,
          y: 0.0,
        },
      ],
      Vec::new(),
    );
    let normal = compositor.motion_lean_degrees(75_000, 32.0);
    let giant = compositor.motion_lean_degrees(75_000, 160.0);
    assert!(giant < normal * 0.5, "giant={giant}, normal={normal}");
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
    assert_eq!(compositor.motion_lean_degrees(500_000, 32.0), 0.0);
  }

  #[test]
  fn tiny_cursor_corrections_become_one_stationary_average() {
    let positions = [
      (0, 500.0, 500.0),
      (100_000, 505.0, 493.0),
      (200_000, 510.0, 500.0),
    ]
    .into_iter()
    .map(|(timestamp_us, x, y)| Position {
      segment: 0,
      timestamp_us,
      x,
      y,
    })
    .collect::<Vec<_>>();
    let compositor = compositor(stabilise_positions(&positions, 1_000.0), Vec::new());
    let midpoint = compositor.smoothed_position(100_000, true).unwrap();
    assert!((midpoint.x - 505.0).abs() < 0.001);
    assert!((midpoint.y - 500.0).abs() < 0.001);
    let endpoint = compositor.smoothed_position(200_000, true).unwrap();
    assert!((endpoint.x - 505.0).abs() < 0.001);
    assert!((endpoint.y - 500.0).abs() < 0.001);
  }

  #[test]
  fn disabling_smoothing_uses_uncollapsed_cursor_samples() {
    let raw_positions = [(0, 0.0), (100_000, 5.0), (200_000, 10.0)]
      .into_iter()
      .map(|(timestamp_us, x)| Position {
        segment: 0,
        timestamp_us,
        x,
        y: 0.0,
      })
      .collect::<Vec<_>>();
    let mut compositor = compositor(raw_positions.clone(), Vec::new());
    compositor.positions = stabilise_positions(&raw_positions, compositor.source.width);

    assert_eq!(compositor.smoothed_position(0, false).unwrap().x, 0.0);
    assert_eq!(compositor.smoothed_position(100_000, false).unwrap().x, 5.0);
    assert_eq!(
      compositor.smoothed_position(200_000, false).unwrap().x,
      10.0
    );
    assert!((compositor.smoothed_position(0, true).unwrap().x - 5.0).abs() < 0.001);
  }

  #[test]
  fn windows_dwell_is_an_exact_anchor_with_a_smooth_approach() {
    let positions = vec![
      Position {
        segment: 0,
        timestamp_us: 0,
        x: 0.0,
        y: 0.0,
      },
      Position {
        segment: 0,
        timestamp_us: 100_000,
        x: 100.0,
        y: 50.0,
      },
      Position {
        segment: 0,
        timestamp_us: 500_000,
        x: 200.0,
        y: 80.0,
      },
    ];
    let mut compositor = compositor(positions.clone(), Vec::new());
    compositor.dwell_anchors = dwell_anchors(&positions);

    let held = compositor.smoothed_position(300_000, true).unwrap();
    assert_eq!((held.x, held.y), (100.0, 50.0));
    let arrival = compositor.smoothed_position(100_000, true).unwrap();
    assert_eq!((arrival.x, arrival.y), (100.0, 50.0));
    let approaching = compositor.smoothed_position(75_000, true).unwrap();
    assert!(approaching.x < 100.0);
    assert!(approaching.x > 75.0);
  }

  #[test]
  fn sparse_windows_stationary_drift_is_held_still() {
    let positions = [
      (2_397_216, 1_384.0, 457.0),
      (3_029_907, 1_403.0, 459.0),
      (3_186_238, 1_402.0, 459.0),
      (3_724_213, 1_401.0, 459.0),
      (4_394_084, 1_402.0, 459.0),
      (5_035_643, 1_403.0, 457.0),
      (5_601_508, 1_400.0, 457.0),
      (6_394_144, 1_403.0, 458.0),
      (6_598_040, 1_404.0, 460.0),
    ]
    .into_iter()
    .map(|(timestamp_us, x, y)| Position {
      segment: 0,
      timestamp_us,
      x,
      y,
    })
    .collect::<Vec<_>>();
    let compositor = compositor(stabilise_positions(&positions, 1_920.0), Vec::new());
    let early = compositor.smoothed_position(3_500_000, true).unwrap();
    let late = compositor.smoothed_position(6_200_000, true).unwrap();

    assert!((early.x - late.x).abs() < 0.001);
    assert!((early.y - late.y).abs() < 0.001);
    assert_eq!(compositor.motion_lean_degrees(5_000_000, 32.0), 0.0);
  }
}
