// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Axis } from "./gradient-field";
import { DistanceProbe, Point } from "./pixel-analysis";
import { Guide, Measurement } from "./ruler-types";

/**
 * An axis-`x` probe is a HORIZONTAL ray at `position` walking along x; axis `y`
 * is the vertical mirror. `along` is the coordinate the ray travels, `across`
 * the one it sits at.
 */
const alongOf = (axis: Axis, point: Point) =>
  axis === "x" ? point.x : point.y;

/**
 * World coordinates where the ray meets committed artifacts. Guides on the same
 * axis are full-length lines, so they always cross; a box edge only counts when
 * the ray actually passes through that edge's segment.
 */
const stopsFor = ({
  axis,
  guides,
  measurements,
  position,
}: {
  axis: Axis;
  guides: readonly Guide[];
  measurements: readonly Measurement[];
  position: number;
}) => {
  const stops: number[] = [];
  for (const guide of guides)
    if (guide.axis === axis) stops.push(guide.position);
  for (const box of measurements) {
    const acrossStart = axis === "x" ? box.y : box.x;
    const acrossSize = axis === "x" ? box.height : box.width;
    if (position < acrossStart || position > acrossStart + acrossSize) continue;
    const alongStart = axis === "x" ? box.x : box.y;
    const alongSize = axis === "x" ? box.width : box.height;
    stops.push(alongStart, alongStart + alongSize);
  }
  return stops;
};

/** A stop this close to the cursor is the line it stands ON, not a target. */
const ORIGIN_SLACK = 1;

/**
 * Tightens a pixel-edge probe so it also stops at committed guides and
 * measurement-box edges. Stops can only shorten the span, so taking the extreme
 * on each side of the cursor is the same as picking the nearest stop. A stop at
 * the cursor itself clips neither side — probing while standing on a guide
 * measures outward FROM it instead of collapsing to zero.
 */
export const clipProbe = ({
  cursor,
  guides,
  measurements,
  probe,
}: {
  cursor: Point;
  guides: readonly Guide[];
  measurements: readonly Measurement[];
  probe: DistanceProbe;
}): DistanceProbe => {
  const origin = alongOf(probe.axis, cursor);
  let { end, start } = probe;
  for (const stop of stopsFor({
    axis: probe.axis,
    guides,
    measurements,
    position: probe.position,
  })) {
    if (stop <= origin - ORIGIN_SLACK) start = Math.max(start, stop);
    else if (stop >= origin + ORIGIN_SLACK) end = Math.min(end, stop);
  }
  return { ...probe, end, start };
};
