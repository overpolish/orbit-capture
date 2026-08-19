// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Region } from "../recording-sources/types";

export const wholePixel = (value: number) => Math.round(value);
export const wholePixelSize = (value: number) => Math.max(1, wholePixel(value));

/** The "no region yet" region a screenshot session starts from. */
export const EMPTY_REGION: Region = {
  position: { x: 0, y: 0 },
  size: { height: 0, width: 0 },
};

/** Whether a region has been drawn at all, rather than left empty. */
export const hasRegion = (region: Region) =>
  region.size.width > 0 && region.size.height > 0;

/** The region on whole-pixel boundaries, as capture and storage want it. */
export const snapRegion = (region: Region): Region => ({
  position: {
    x: wholePixel(region.position.x),
    y: wholePixel(region.position.y),
  },
  size: {
    height: wholePixelSize(region.size.height),
    width: wholePixelSize(region.size.width),
  },
});

const clamp = (value: number, maximum: number) =>
  Math.max(0, Math.min(value, maximum));

export const drawnRegion = ({
  aspect,
  bounds,
  end,
  start,
}: {
  bounds: { height: number; width: number };
  end: { x: number; y: number };
  start: { x: number; y: number };
  /** Width over height the drawn rect is held to, when one is locked. */
  aspect?: number;
}): Region => {
  const startX = clamp(start.x, bounds.width);
  const startY = clamp(start.y, bounds.height);
  const endX = clamp(end.x, bounds.width);
  const endY = clamp(end.y, bounds.height);
  let width = Math.abs(endX - startX);
  let height = Math.abs(endY - startY);

  if (aspect !== undefined && aspect > 0) {
    // The dominant axis of the drag leads, the other follows the ratio.
    const dominatesWidth = (height === 0 ? Infinity : width / height) > aspect;
    const ratioWidth = dominatesWidth ? width : height * aspect;
    const ratioHeight = dominatesWidth ? width / aspect : height;
    // The rect grows away from the start point, so the room it has is
    // whatever lies between that point and the edge it grows towards.
    const availableWidth = endX < startX ? startX : bounds.width - startX;
    const availableHeight = endY < startY ? startY : bounds.height - startY;
    // Shrink rather than crop, so hitting an edge keeps the ratio.
    const fit = Math.min(
      1,
      ratioWidth > 0 ? availableWidth / ratioWidth : 1,
      ratioHeight > 0 ? availableHeight / ratioHeight : 1,
    );
    width = ratioWidth * fit;
    height = ratioHeight * fit;
  }

  const regionWidth = Math.min(bounds.width, wholePixelSize(width));
  const regionHeight = Math.min(bounds.height, wholePixelSize(height));
  const left = endX < startX ? startX - width : startX;
  const top = endY < startY ? startY - height : startY;

  return {
    position: {
      x: wholePixel(clamp(left, bounds.width - regionWidth)),
      y: wholePixel(clamp(top, bounds.height - regionHeight)),
    },
    size: { height: regionHeight, width: regionWidth },
  };
};

export const fitRegion = (
  region: Region,
  width: number,
  height: number,
): Region => {
  const margin = 20;
  const fittedWidth = wholePixelSize(
    Math.min(region.size.width, width - margin),
  );
  const fittedHeight = wholePixelSize(
    Math.min(region.size.height, height - margin),
  );
  return {
    position: {
      x: wholePixel(
        Math.max(0, Math.min(region.position.x, width - fittedWidth)),
      ),
      y: wholePixel(
        Math.max(0, Math.min(region.position.y, height - fittedHeight)),
      ),
    },
    size: { height: fittedHeight, width: fittedWidth },
  };
};
