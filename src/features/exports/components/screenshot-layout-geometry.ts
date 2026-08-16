// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { TransformEdge } from "../../../components/shared/canvas-tools/transform-handles";
import { clamp, OverlayRect } from "../camera-overlay-geometry";

export type ScreenshotLayoutEdge = TransformEdge;

export const constrainedHandlePoint = (
  crop: OverlayRect,
  edges: ScreenshotLayoutEdge[],
  pointer: { x: number; y: number },
) => ({
  x: edges.includes("left")
    ? crop.x
    : edges.includes("right")
      ? crop.x + crop.width
      : clamp(pointer.x, crop.x, crop.x + crop.width),
  y: edges.includes("top")
    ? crop.y
    : edges.includes("bottom")
      ? crop.y + crop.height
      : clamp(pointer.y, crop.y, crop.y + crop.height),
});

export const cropEdgesInsideImage = (
  image: OverlayRect,
  crop: OverlayRect,
  minimum: number,
) => {
  const imageRight = image.x + image.width;
  const imageBottom = image.y + image.height;
  const minimumWidth = Math.min(minimum, Math.max(0, image.width));
  const minimumHeight = Math.min(minimum, Math.max(0, image.height));
  const left = clamp(crop.x, image.x, imageRight - minimumWidth);
  const top = clamp(crop.y, image.y, imageBottom - minimumHeight);
  return {
    bottom: clamp(crop.y + crop.height, top + minimumHeight, imageBottom),
    imageBottom,
    imageLeft: image.x,
    imageRight,
    imageTop: image.y,
    left,
    minimumHeight,
    minimumWidth,
    right: clamp(crop.x + crop.width, left + minimumWidth, imageRight),
    top,
  };
};
