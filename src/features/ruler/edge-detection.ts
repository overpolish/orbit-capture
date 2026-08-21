// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Axis,
  axisLength,
  buildProfile,
  GradientField,
  gradientAt,
  nearestPeak,
  profilePeaks,
} from "./gradient-field";

import type { Point } from "./pixel-analysis";

const DEFAULT_RADIUS = 20;
const NEIGHBOUR_HITS = 2;

/**
 * Nearest gradient ridge to `target` inside the search window. Nearest rather
 * than strongest, because the drag already told us roughly where the edge is
 * and the strongest ridge in a 40 px window is usually a different element.
 */
export const snapEdge = (
  field: GradientField,
  {
    axis,
    rangeEnd,
    rangeStart,
    searchAfter,
    searchBefore,
    target,
    threshold,
  }: {
    axis: Axis;
    rangeEnd: number;
    rangeStart: number;
    target: number;
    threshold: number;
    searchAfter?: number;
    searchBefore?: number;
  },
) => {
  const before = Math.round(searchBefore ?? DEFAULT_RADIUS);
  const after = Math.round(searchAfter ?? DEFAULT_RADIUS);
  const profile = buildProfile(field, {
    axis,
    from: target - before,
    rangeEnd,
    rangeStart,
    to: target + after,
  });
  const peak = nearestPeak(profilePeaks(profile, threshold), target);
  return peak === undefined ? target : peak.position;
};

/**
 * Edge mass at `position` along the walk axis: the ridge plus half of each
 * immediate neighbour. Anti-aliasing splits an edge's contrast across two or
 * three pixels, so the raw per-pixel value understates a subtle edge; summing
 * along the walk axis recovers it while leaving hard 1 px ridges at their true
 * contrast.
 */
const edgeMass = (
  field: GradientField,
  { across, axis, position }: { across: number; axis: Axis; position: number },
) =>
  gradientAt(field, { across, axis, position }) +
  (gradientAt(field, { across, axis, position: position - 1 }) +
    gradientAt(field, { across, axis, position: position + 1 })) /
    2;

/**
 * A probe only stops where at least two of the three perpendicular neighbours
 * are gradient hits, so single-pixel text speckle does not terminate the walk.
 */
const isEdge = (
  field: GradientField,
  {
    across,
    axis,
    position,
    threshold,
  }: { across: number; axis: Axis; position: number; threshold: number },
) => {
  let hits = 0;
  for (let offset = -1; offset <= 1; offset += 1) {
    const at = { across: across + offset, axis, position };
    // A flat pixel next to a strong ridge borrows enough mass to pass on its
    // own, which would stop every probe one pixel short of a hard edge — the
    // pixel itself must carry gradient, exactly as the Rust binarization does.
    if (gradientAt(field, at) > 0 && edgeMass(field, at) >= threshold)
      hits += 1;
  }
  return hits >= NEIGHBOUR_HITS;
};

/** First gradient hit either side of `point` along `axis`, bounds as fallback. */
export const axisSpanAt = ({
  axis,
  field,
  point,
  threshold,
}: {
  axis: Axis;
  field: GradientField;
  point: Point;
  threshold: number;
}) => {
  const target = Math.round(axis === "x" ? point.x : point.y);
  const across = Math.round(axis === "x" ? point.y : point.x);
  const limit = axisLength(field, axis) - 1;
  const find = (direction: -1 | 1) => {
    for (
      let position = direction < 0 ? target : target + 1;
      position > 0 && position <= limit;
      position += direction
    )
      if (isEdge(field, { across, axis, position, threshold })) return position;
    return direction < 0 ? 0 : limit;
  };
  return { across, end: find(1), start: find(-1) };
};
