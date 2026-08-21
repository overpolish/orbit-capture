// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Axis,
  axisLength,
  buildProfile,
  crossAxis,
  GradientField,
  nearestPeak,
  Peak,
  profilePeaks,
} from "./gradient-field";

import type { Point } from "./pixel-analysis";

/** Perpendicular strip, in physical px, that feeds the local profile. */
const STRIP = 20;
/** Search radius in physical px at zoom 1; tightened as the view zooms in. */
const BASE_RADIUS = 20;
/** A rival ridge has to be this much better before it steals the guide. */
const BEAT_RATIO = 1.2;
/** A rival ridge must be this much nearer the cursor to steal the guide. */
const NEARER_MARGIN = 3;

export type SnappedGuide = { position: number; score: number };

/**
 * Weights a local ridge by how far it runs across the whole screen, so window
 * borders and layout dividers outrank equally contrasty local speckle.
 */
const withProjectionSupport = (
  field: GradientField,
  { axis, peak }: { axis: Axis; peak: Peak },
): Peak => {
  const projection = axis === "x" ? field.colSum : field.rowSum;
  const length = Math.max(1, axisLength(field, crossAxis(axis)));
  const support =
    peak.position >= 0 && peak.position < projection.length
      ? projection[peak.position] / length
      : 0;
  return {
    position: peak.position,
    score: peak.score * (1 + Math.log1p(support)),
  };
};

const candidatesAt = (
  field: GradientField,
  {
    axis,
    point,
    radius,
    threshold,
  }: { axis: Axis; point: Point; radius: number; threshold: number },
) => {
  const position = axis === "x" ? point.x : point.y;
  const across = axis === "x" ? point.y : point.x;
  const profile = buildProfile(field, {
    axis,
    from: position - radius,
    rangeEnd: across + STRIP,
    rangeStart: across - STRIP,
    to: position + radius,
  });
  return profilePeaks(profile, threshold).map((peak) =>
    withProjectionSupport(field, { axis, peak }),
  );
};

/**
 * Picks the ridge nearest the cursor, but keeps the previously held one unless
 * the newcomer clearly wins — otherwise two rival edges flicker under a jitter
 * of a single pixel.
 */
export const snapGuide = (
  field: GradientField,
  {
    axis,
    point,
    previous,
    threshold,
    zoom,
  }: {
    axis: Axis;
    point: Point;
    threshold: number;
    zoom: number;
    previous?: SnappedGuide;
  },
): SnappedGuide | undefined => {
  const radius = Math.max(2, Math.round(BASE_RADIUS / Math.max(1, zoom)));
  const candidates = candidatesAt(field, { axis, point, radius, threshold });
  const target = axis === "x" ? point.x : point.y;
  const next = nearestPeak(candidates, target);
  if (next === undefined) return undefined;
  if (previous === undefined || previous.position === next.position)
    return next;
  const held = candidates.find(
    ({ position }) => position === previous.position,
  );
  if (held === undefined || next.score >= held.score * BEAT_RATIO) return next;
  const nearer =
    Math.abs(target - next.position) + NEARER_MARGIN <=
    Math.abs(target - previous.position);
  return nearer ? next : held;
};
