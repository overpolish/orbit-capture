// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { OverlayRect } from "./camera-overlay-geometry";

export type ScreenshotSnapGuide = {
  source: "canvas" | "object";
  value: number;
};

const CANVAS_INSET_PERCENT = 2;

const anchors = (origin: number, size: number) => [
  origin,
  origin + size / 2,
  origin + size,
];

const nearestObjectAdjustment = ({
  moving,
  objects,
  threshold,
}: {
  moving: number[];
  objects: number[];
  threshold: number;
}) => {
  let best:
    | { adjustment: number; distance: number; guide: ScreenshotSnapGuide }
    | undefined;
  for (const movingValue of moving) {
    for (const target of objects) {
      const adjustment = target - movingValue;
      const distance = Math.abs(adjustment);
      if (distance <= threshold && (!best || distance < best.distance)) {
        best = {
          adjustment,
          distance,
          guide: { source: "object", value: target },
        };
      }
    }
  }
  return best;
};

const nearestCanvasPlacement = ({
  canvasSize,
  frameSize,
  inset,
  position,
  threshold,
}: {
  canvasSize: number;
  frameSize: number;
  inset: number;
  position: number;
  threshold: number;
}) => {
  const maximum = canvasSize - frameSize;
  const placements =
    maximum >= 0
      ? [Math.min(inset, maximum), maximum / 2, Math.max(0, maximum - inset)]
      : [0, maximum / 2, maximum];
  const placement = placements.reduce((nearest, candidate) =>
    Math.abs(candidate - position) < Math.abs(nearest - position)
      ? candidate
      : nearest,
  );
  const placementIndex = placements.indexOf(placement);
  const distance = Math.abs(placement - position);
  if (distance > threshold) return undefined;
  return {
    adjustment: placement - position,
    distance,
    guide: {
      source: "canvas" as const,
      value:
        placementIndex === 0
          ? placement
          : placementIndex === 1
            ? canvasSize / 2
            : placement + frameSize,
    },
  };
};

/** Threshold snapping shared by screenshot layers and future canvas objects. */
export const snapScreenshotFrame = ({
  canvas,
  frame,
  objects,
  position,
  thresholdX,
  thresholdY,
}: {
  canvas: { height: number; width: number };
  frame: OverlayRect;
  objects: OverlayRect[];
  position: { x: number; y: number };
  thresholdX: number;
  thresholdY: number;
}) => {
  const inset =
    (Math.min(canvas.width, canvas.height) * CANVAS_INSET_PERCENT) / 100;
  const canvasX = nearestCanvasPlacement({
    canvasSize: canvas.width,
    frameSize: frame.width,
    inset,
    position: position.x,
    threshold: thresholdX,
  });
  const canvasY = nearestCanvasPlacement({
    canvasSize: canvas.height,
    frameSize: frame.height,
    inset,
    position: position.y,
    threshold: thresholdY,
  });
  const objectX = nearestObjectAdjustment({
    moving: anchors(position.x, frame.width),
    objects: objects.flatMap((object) => anchors(object.x, object.width)),
    threshold: thresholdX,
  });
  const objectY = nearestObjectAdjustment({
    moving: anchors(position.y, frame.height),
    objects: objects.flatMap((object) => anchors(object.y, object.height)),
    threshold: thresholdY,
  });
  const nearest = <T extends { distance: number }>(
    first: T | undefined,
    second: T | undefined,
  ) =>
    first && second
      ? first.distance <= second.distance
        ? first
        : second
      : (first ?? second);
  const x = nearest(objectX, canvasX);
  const y = nearest(objectY, canvasY);
  return {
    guides: { x: x?.guide, y: y?.guide },
    position: {
      x: position.x + (x?.adjustment ?? 0),
      y: position.y + (y?.adjustment ?? 0),
    },
  };
};
