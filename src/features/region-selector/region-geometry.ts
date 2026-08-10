// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Region } from "../recording-sources/types";

export const wholePixel = (value: number) => Math.round(value);
export const wholePixelSize = (value: number) => Math.max(1, wholePixel(value));

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
