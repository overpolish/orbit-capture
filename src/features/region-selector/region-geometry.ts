// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Region } from "../recording-sources/types";

export const wholePixel = (value: number) => Math.round(value);
export const wholePixelSize = (value: number) => Math.max(1, wholePixel(value));

const clamp = (value: number, maximum: number) =>
  Math.max(0, Math.min(value, maximum));

export const drawnRegion = ({
  bounds,
  end,
  start,
}: {
  bounds: { height: number; width: number };
  end: { x: number; y: number };
  start: { x: number; y: number };
}): Region => {
  const startX = clamp(start.x, bounds.width);
  const startY = clamp(start.y, bounds.height);
  const endX = clamp(end.x, bounds.width);
  const endY = clamp(end.y, bounds.height);
  const width = Math.abs(endX - startX);
  const height = Math.abs(endY - startY);

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
