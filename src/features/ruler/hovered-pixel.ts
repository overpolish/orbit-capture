// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  colorAt,
  PixelSize,
  PixelSnapshot,
  Point,
  screenToPixel,
} from "./pixel-analysis";

/**
 * Color of the snapshot pixel under the cursor. Stays undefined until the
 * capture has loaded and the pointer has been somewhere.
 */
export function hoveredPixelAt({
  cursor,
  snapshot,
  viewport,
}: {
  viewport: PixelSize;
  cursor?: Point;
  snapshot?: PixelSnapshot;
}) {
  const pixelPoint =
    snapshot && cursor ? screenToPixel(cursor, snapshot, viewport) : undefined;
  return {
    hoveredColor:
      snapshot && pixelPoint ? colorAt(snapshot, pixelPoint) : undefined,
  };
}
