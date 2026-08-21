// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RulerComponentBox } from "./api";
import { boxRect, contains, PixelRect } from "./bounds-snap";
import { Axis } from "./gradient-field";
import { Bounds } from "./pixel-analysis";

/** Centres this close in device px read as collinear. */
const ALIGNMENT_SLACK = 1;
/** Below this a component is noise — antialiasing, a hairline, a stray dot. */
const MINIMUM_OBJECT_SIZE = 3;
/** More outlines than this stops reading as "the things in the box". */
const MAXIMUM_OBJECTS = 12;

export type Centerline = {
  accent: boolean;
  position: number;
};

/** A piece of content inside a measurement, in world coordinates. */
export type InnerObject = {
  alignedX: boolean;
  alignedY: boolean;
  bounds: Bounds;
};

export type Centerlines = {
  objects: readonly InnerObject[];
  x: Centerline;
  y: Centerline;
};

const centreOf = (bounds: Bounds) => ({
  x: bounds.x + bounds.width / 2,
  y: bounds.y + bounds.height / 2,
});

/** A measurement that snapped onto a component matches it edge-for-edge. */
const SELF_SLACK = 3;

const isSelf = (rect: PixelRect, box: PixelRect) =>
  Math.abs(box.x0 - rect.x0) <= SELF_SLACK &&
  Math.abs(box.y0 - rect.y0) <= SELF_SLACK &&
  Math.abs(box.x1 - rect.x1) <= SELF_SLACK &&
  Math.abs(box.y1 - rect.y1) <= SELF_SLACK;

/**
 * What constitutes ONE object: components whose boxes come within this many
 * device px of each other cluster together, so a multi-stroke icon (or the
 * words of a label) reads as a single object instead of its parts.
 */
const CLUSTER_GAP = 6;
/** Clustering input cap — keeps the merge loop bounded on text-heavy boxes. */
const MAXIMUM_PARTS = 128;

const near = (a: PixelRect, b: PixelRect) =>
  a.x0 - CLUSTER_GAP <= b.x1 &&
  b.x0 - CLUSTER_GAP <= a.x1 &&
  a.y0 - CLUSTER_GAP <= b.y1 &&
  b.y0 - CLUSTER_GAP <= a.y1;

const merge = (a: PixelRect, b: PixelRect): PixelRect => ({
  x0: Math.min(a.x0, b.x0),
  x1: Math.max(a.x1, b.x1),
  y0: Math.min(a.y0, b.y0),
  y1: Math.max(a.y1, b.y1),
});

/** Repeatedly merges near rects; ends with disjoint, well-separated clusters. */
const clustered = (parts: readonly PixelRect[]): PixelRect[] => {
  const clusters = [...parts];
  let changed = true;
  while (changed) {
    changed = false;
    outer: for (let a = 0; a < clusters.length; a += 1) {
      for (let b = a + 1; b < clusters.length; b += 1) {
        if (!near(clusters[a], clusters[b])) continue;
        clusters[a] = merge(clusters[a], clusters[b]);
        clusters.splice(b, 1);
        changed = true;
        break outer;
      }
    }
  }
  return clusters;
};

/**
 * The objects INSIDE the measurement whose centring is worth reporting: the
 * icon, the label, the badge. Detector components sitting within
 * [`CLUSTER_GAP`] of each other are one visual object — a multi-stroke icon
 * must not read as its strokes. The component (or cluster) matching the
 * measurement itself is excluded — comparing a box against itself is always
 * "centred" and says nothing — as are specks.
 */
const innerObjects = (
  boxes: readonly RulerComponentBox[],
  rect: PixelRect,
): readonly PixelRect[] => {
  const inside: { area: number; box: PixelRect }[] = [];
  for (const box of boxes) {
    const candidate = boxRect(box);
    if (!contains(rect, candidate) || isSelf(rect, candidate)) continue;
    const width = candidate.x1 - candidate.x0;
    const height = candidate.y1 - candidate.y0;
    if (width < MINIMUM_OBJECT_SIZE || height < MINIMUM_OBJECT_SIZE) continue;
    inside.push({ area: width * height, box: candidate });
  }
  inside.sort((a, b) => b.area - a.area);
  // Clustering also swallows nested parts: containment implies nearness.
  const clusters = clustered(
    inside.slice(0, MAXIMUM_PARTS).map(({ box }) => box),
  );
  return clusters
    .filter((cluster) => !isSelf(rect, cluster))
    .sort(
      (a, b) => (b.x1 - b.x0) * (b.y1 - b.y0) - (a.x1 - a.x0) * (a.y1 - a.y0),
    )
    .slice(0, MAXIMUM_OBJECTS);
};

const unionOf = (rects: readonly PixelRect[]) =>
  rects.reduce<PixelRect | undefined>(
    (union, rect) =>
      union === undefined
        ? rect
        : {
            x0: Math.min(union.x0, rect.x0),
            x1: Math.max(union.x1, rect.x1),
            y0: Math.min(union.y0, rect.y0),
            y1: Math.max(union.y1, rect.y1),
          },
    undefined,
  );

const rectCentre = (rect: PixelRect, axis: Axis) =>
  axis === "x" ? (rect.x0 + rect.x1) / 2 : (rect.y0 + rect.y1) / 2;

export const centerlines = ({
  bounds,
  boxes,
  deviceScale,
  peers,
}: {
  bounds: Bounds;
  boxes: readonly RulerComponentBox[];
  deviceScale: number;
  peers: readonly Bounds[];
}): Centerlines => {
  const centre = centreOf(bounds);
  const objects = innerObjects(boxes, {
    x0: bounds.x * deviceScale,
    x1: (bounds.x + bounds.width) * deviceScale,
    y0: bounds.y * deviceScale,
    y1: (bounds.y + bounds.height) * deviceScale,
  });
  const union = unionOf(objects);
  const alignedOn = (rect: PixelRect, axis: Axis) => {
    const position = axis === "x" ? centre.x : centre.y;
    const delta = rectCentre(rect, axis) - position * deviceScale;
    return Math.abs(delta) <= ALIGNMENT_SLACK;
  };
  const axisLine = (axis: Axis): Centerline => {
    const position = axis === "x" ? centre.x : centre.y;
    const peerAligned = peers.some((peer) => {
      const other = axis === "x" ? centreOf(peer).x : centreOf(peer).y;
      return Math.abs(other - position) * deviceScale <= ALIGNMENT_SLACK;
    });
    const unionAligned = union !== undefined && alignedOn(union, axis);
    return { accent: peerAligned || unionAligned, position };
  };
  return {
    objects: objects.map((rect) => ({
      alignedX: alignedOn(rect, "x"),
      alignedY: alignedOn(rect, "y"),
      bounds: {
        height: (rect.y1 - rect.y0) / deviceScale,
        width: (rect.x1 - rect.x0) / deviceScale,
        x: rect.x0 / deviceScale,
        y: rect.y0 / deviceScale,
      },
    })),
    x: axisLine("x"),
    y: axisLine("y"),
  };
};
