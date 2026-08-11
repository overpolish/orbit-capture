// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::recording::cursor::CursorStyle;

type Point = (f64, f64);
type Line = (Point, Point);

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Artwork {
  Arrow,
  IBeam,
  Resize,
}

pub(super) fn artwork(style: CursorStyle) -> Artwork {
  match style {
    CursorStyle::IBeam | CursorStyle::VerticalIBeam => Artwork::IBeam,
    CursorStyle::ResizeHorizontal | CursorStyle::ResizeVertical => Artwork::Resize,
    _ => Artwork::Arrow,
  }
}

pub(super) fn is_vertical(style: CursorStyle) -> bool {
  matches!(
    style,
    CursorStyle::ResizeVertical | CursorStyle::VerticalIBeam
  )
}

pub(super) fn sample(artwork: Artwork, x: f64, y: f64) -> [f64; 4] {
  match artwork {
    Artwork::Arrow => {
      let edge = polygon_distance((x, y), &ARROW_POINTS);
      if edge <= 1.25 {
        [0.0, 0.0, 0.0, 255.0]
      } else if point_in_polygon((x, y), &ARROW_POINTS) {
        [255.0, 255.0, 255.0, 255.0]
      } else {
        [0.0; 4]
      }
    }
    Artwork::IBeam => sample_stroked_lines((x, y), &I_BEAM_LINES),
    Artwork::Resize => sample_stroked_lines((x, y), &RESIZE_LINES),
  }
}

const ARROW_POINTS: [Point; 7] = [
  (3.0, 2.0),
  (3.0, 31.0),
  (10.4, 24.0),
  (15.9, 37.0),
  (21.0, 34.8),
  (15.5, 22.2),
  (26.0, 22.2),
];
const I_BEAM_LINES: [Line; 3] = [
  ((8.0, 3.0), (20.0, 3.0)),
  ((14.0, 3.0), (14.0, 37.0)),
  ((8.0, 37.0), (20.0, 37.0)),
];
const RESIZE_LINES: [Line; 5] = [
  ((2.0, 20.0), (26.0, 20.0)),
  ((2.0, 20.0), (9.0, 13.0)),
  ((2.0, 20.0), (9.0, 27.0)),
  ((26.0, 20.0), (19.0, 13.0)),
  ((26.0, 20.0), (19.0, 27.0)),
];

fn sample_stroked_lines(point: Point, lines: &[Line]) -> [f64; 4] {
  let distance = lines
    .iter()
    .map(|(start, end)| segment_distance(point, *start, *end))
    .fold(f64::INFINITY, f64::min);
  if distance <= 1.25 {
    [255.0, 255.0, 255.0, 255.0]
  } else if distance <= 2.5 {
    [0.0, 0.0, 0.0, 255.0]
  } else {
    [0.0; 4]
  }
}

fn polygon_distance(point: Point, polygon: &[Point]) -> f64 {
  polygon
    .iter()
    .copied()
    .zip(polygon.iter().copied().cycle().skip(1))
    .take(polygon.len())
    .map(|(start, end)| segment_distance(point, start, end))
    .fold(f64::INFINITY, f64::min)
}

fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
  polygon
    .iter()
    .copied()
    .zip(polygon.iter().copied().cycle().skip(1))
    .take(polygon.len())
    .fold(false, |inside, (start, end)| {
      let crosses = (start.1 > point.1) != (end.1 > point.1)
        && point.0 < (end.0 - start.0) * (point.1 - start.1) / (end.1 - start.1) + start.0;
      inside ^ crosses
    })
}

fn segment_distance(point: Point, start: Point, end: Point) -> f64 {
  let line_x = end.0 - start.0;
  let line_y = end.1 - start.1;
  let length_squared = line_x * line_x + line_y * line_y;
  let progress = if length_squared > 0.0 {
    ((point.0 - start.0) * line_x + (point.1 - start.1) * line_y) / length_squared
  } else {
    0.0
  }
  .clamp(0.0, 1.0);
  let closest_x = start.0 + line_x * progress;
  let closest_y = start.1 + line_y * progress;
  (point.0 - closest_x).hypot(point.1 - closest_y)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn vector_arrow_has_fill_stroke_and_transparency() {
    assert_eq!(sample(Artwork::Arrow, 7.0, 15.0), [255.0; 4]);
    assert_eq!(sample(Artwork::Arrow, 3.0, 15.0), [0.0, 0.0, 0.0, 255.0]);
    assert_eq!(sample(Artwork::Arrow, 27.0, 2.0), [0.0; 4]);
  }
}
