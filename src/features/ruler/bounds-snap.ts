// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RulerComponentBox } from "./api";
import { snapEdge } from "./edge-detection";
import { GradientField } from "./gradient-field";
import { Bounds, PixelSize, screenToPixel } from "./pixel-analysis";

/** How far outside the drag a component may sit and still count as circled. */
const CONTAINMENT_SLACK = 12;
/** Overlap at which a single component is treated as the intended container. */
const MINIMUM_IOU = 0.25;
const EDGE_SEARCH = 20;

/** Half-open physical-pixel rectangle, matching the detector's box convention. */
export type PixelRect = { x0: number; x1: number; y0: number; y1: number };

export const boxRect = (box: RulerComponentBox): PixelRect => ({
  x0: box.x,
  x1: box.x + box.width,
  y0: box.y,
  y1: box.y + box.height,
});

const rectArea = (rect: PixelRect) =>
  Math.max(0, rect.x1 - rect.x0) * Math.max(0, rect.y1 - rect.y0);

const intersectionOver = (a: PixelRect, b: PixelRect) => {
  const overlap = rectArea({
    x0: Math.max(a.x0, b.x0),
    x1: Math.min(a.x1, b.x1),
    y0: Math.max(a.y0, b.y0),
    y1: Math.min(a.y1, b.y1),
  });
  const union = rectArea(a) + rectArea(b) - overlap;
  return union > 0 ? overlap / union : 0;
};

export const contains = (outer: PixelRect, inner: PixelRect) =>
  inner.x0 >= outer.x0 &&
  inner.y0 >= outer.y0 &&
  inner.x1 <= outer.x1 &&
  inner.y1 <= outer.y1;

/** "Measure what I circled": union of every component inside the loose drag. */
const containedUnion = (
  boxes: readonly RulerComponentBox[],
  drag: PixelRect,
) => {
  const grown: PixelRect = {
    x0: drag.x0 - CONTAINMENT_SLACK,
    x1: drag.x1 + CONTAINMENT_SLACK,
    y0: drag.y0 - CONTAINMENT_SLACK,
    y1: drag.y1 + CONTAINMENT_SLACK,
  };
  let union: PixelRect | undefined;
  for (const box of boxes) {
    const rect = boxRect(box);
    if (!contains(grown, rect)) continue;
    union =
      union === undefined
        ? rect
        : {
            x0: Math.min(union.x0, rect.x0),
            x1: Math.max(union.x1, rect.x1),
            y0: Math.min(union.y0, rect.y0),
            y1: Math.max(union.y1, rect.y1),
          };
  }
  return union;
};

/** The drag approximates one container: take the component it best matches. */
const bestOverlap = (boxes: readonly RulerComponentBox[], drag: PixelRect) => {
  let best: PixelRect | undefined;
  let bestScore = 0;
  for (const box of boxes) {
    const rect = boxRect(box);
    const score = intersectionOver(rect, drag);
    if (score > bestScore) {
      best = rect;
      bestScore = score;
    }
  }
  return bestScore >= MINIMUM_IOU ? best : undefined;
};

/** Last resort: pull each edge onto the nearest ridge within a small window. */
const snappedEdges = (
  field: GradientField,
  drag: PixelRect,
  threshold: number,
): PixelRect => {
  const edge = (options: {
    axis: "x" | "y";
    rangeEnd: number;
    rangeStart: number;
    target: number;
  }) =>
    snapEdge(field, {
      ...options,
      searchAfter: EDGE_SEARCH,
      searchBefore: EDGE_SEARCH,
      threshold,
    });
  const x0 = edge({
    axis: "x",
    rangeEnd: drag.y1,
    rangeStart: drag.y0,
    target: drag.x0,
  });
  const x1 = edge({
    axis: "x",
    rangeEnd: drag.y1,
    rangeStart: drag.y0,
    target: drag.x1,
  });
  const y0 = edge({ axis: "y", rangeEnd: x1, rangeStart: x0, target: drag.y0 });
  const y1 = edge({ axis: "y", rangeEnd: x1, rangeStart: x0, target: drag.y1 });
  return {
    x0: Math.min(x0, x1),
    x1: Math.max(x0, x1),
    y0: Math.min(y0, y1),
    y1: Math.max(y0, y1),
  };
};

/**
 * Cascade from the most intentional reading of a drag to the least: everything
 * circled, then the container it approximates, then bare edge snapping.
 */
export const snapBounds = ({
  bounds,
  boxes,
  field,
  threshold,
  viewport,
}: {
  bounds: Bounds;
  boxes: readonly RulerComponentBox[];
  field: GradientField;
  threshold: number;
  viewport: PixelSize;
}): Bounds => {
  const topLeft = screenToPixel(bounds, field, viewport);
  const bottomRight = screenToPixel(
    { x: bounds.x + bounds.width, y: bounds.y + bounds.height },
    field,
    viewport,
  );
  const drag: PixelRect = {
    x0: topLeft.x,
    x1: Math.max(topLeft.x + 1, bottomRight.x),
    y0: topLeft.y,
    y1: Math.max(topLeft.y + 1, bottomRight.y),
  };
  const result =
    containedUnion(boxes, drag) ??
    bestOverlap(boxes, drag) ??
    snappedEdges(field, drag, threshold);
  const scaleX = viewport.width / field.width;
  const scaleY = viewport.height / field.height;
  return {
    height: (result.y1 - result.y0) * scaleY,
    width: (result.x1 - result.x0) * scaleX,
    x: result.x0 * scaleX,
    y: result.y0 * scaleY,
  };
};
