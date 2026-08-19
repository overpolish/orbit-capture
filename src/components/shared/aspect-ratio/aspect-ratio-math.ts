// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

/** Simplified whole-number aspect ratio units for width and height. */
export type AspectRatioParts = { ratioHeight: number; ratioWidth: number };

/** Returns the greatest common divisor for two numbers. */
const greatestCommonDivisor = (a: number, b: number): number => {
  a = Math.abs(a);
  b = Math.abs(b);
  while (b) {
    const temp = b;
    b = a % b;
    a = temp;
  }
  return a || 1;
};

/** Reduces a width/height pair to the simplest whole-number ratio. */
export const reduceToRatio = (
  width: number,
  height: number,
): AspectRatioParts => {
  const divisor = greatestCommonDivisor(width, height);
  return {
    ratioHeight: Math.round(height / divisor),
    ratioWidth: Math.round(width / divisor),
  };
};

/** Parses a preset id like "16:9" into ratio parts. */
export const parseRatioFromId = (
  id: string | null,
): AspectRatioParts | undefined => {
  if (!id) return undefined;
  const [a, b] = id.split(":").map((n) => Number.parseInt(n, 10));
  if (!Number.isFinite(a) || !Number.isFinite(b) || a <= 0 || b <= 0) {
    return undefined;
  }

  return { ratioHeight: b, ratioWidth: a };
};

/**
 * Whether dimensions already sit on a ratio, bar whole-pixel rounding.
 *
 * Resizing or drawing at a locked ratio lands on whole pixels every frame,
 * which reduces to a slightly different ratio (16:9 becomes 1000:563) and
 * would feed that drift back into the next frame.
 */
export const matchesRatio = (
  width: number,
  height: number,
  ratio: AspectRatioParts,
) => {
  const { ratioHeight, ratioWidth } = ratio;
  if (ratioWidth <= 0 || ratioHeight <= 0) return false;
  return (
    Math.abs(height - (width * ratioHeight) / ratioWidth) <= 1 ||
    Math.abs(width - (height * ratioWidth) / ratioHeight) <= 1
  );
};

export const dimensionsAtRatio = (
  value: number,
  editingDimension: "height" | "width",
  ratio: AspectRatioParts,
) => {
  const { ratioHeight, ratioWidth } = ratio;
  const editedValue = Math.max(1, Math.round(value));
  return editingDimension === "width"
    ? {
        height: Math.max(
          1,
          Math.round((editedValue * ratioHeight) / ratioWidth),
        ),
        width: editedValue,
      }
    : {
        height: editedValue,
        width: Math.max(
          1,
          Math.round((editedValue * ratioWidth) / ratioHeight),
        ),
      };
};

export const closestDimensionsAtRatio = (
  width: number,
  height: number,
  ratio: AspectRatioParts,
) => {
  const fromWidth = dimensionsAtRatio(width, "width", ratio);
  const fromHeight = dimensionsAtRatio(height, "height", ratio);
  const widthDelta =
    Math.abs(fromWidth.width - width) + Math.abs(fromWidth.height - height);
  const heightDelta =
    Math.abs(fromHeight.width - width) + Math.abs(fromHeight.height - height);
  return widthDelta <= heightDelta ? fromWidth : fromHeight;
};
