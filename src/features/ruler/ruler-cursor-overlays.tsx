// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CSSProperties } from "react";

import { Bounds, Point } from "./pixel-analysis";
import { RulerTolerance } from "./ruler-tolerance";
import { DistanceProbe } from "./ruler-types";

/** Tallest chip the flip threshold must clear (the two-line readout). */
const CHIP_CLEARANCE = 56;

/**
 * Places a cursor-following chip below-right of the cursor, flipping above it
 * near the bottom edge. The flipped branch anchors the chip's BOTTOM edge via
 * a translate, so any chip height works without new constants.
 */
const cursorLabelPosition = (
  cursor: Point,
  labelWidth: number,
): CSSProperties => {
  const preferredLeft = cursor.x + 8;
  const flipped = cursor.y + 8 + CHIP_CLEARANCE > window.innerHeight;
  return {
    ...(preferredLeft + labelWidth <= window.innerWidth - 8
      ? { left: Math.max(8, preferredLeft) }
      : { right: 8 }),
    ...(flipped
      ? { top: cursor.y - 8, transform: "translateY(-100%)" }
      : { top: cursor.y + 8 }),
  };
};

export function RulerCrosshair({ cursor }: { cursor: Point }) {
  return (
    <>
      <div
        className="pointer-events-none absolute inset-y-0 w-px bg-error/70"
        style={{ left: cursor.x }}
      />
      <div
        className="pointer-events-none absolute inset-x-0 h-px bg-error/70"
        style={{ top: cursor.y }}
      />
    </>
  );
}

/**
 * One two-line chip: dimensions on top, the pixel colour beneath in a smaller
 * row (Tab copies it — documented in the guides, not here). Either line stands
 * alone when the other has nothing to say.
 */
export function CursorReadout({
  copied,
  cursor,
  draft,
  hex,
  probes,
}: {
  copied: boolean;
  cursor: Point;
  probes: readonly DistanceProbe[];
  draft?: Bounds;
  hex?: string;
}) {
  const horizontal = probes.find(({ axis }) => axis === "x");
  const vertical = probes.find(({ axis }) => axis === "y");
  const dimensions = draft
    ? { height: Math.round(draft.height), width: Math.round(draft.width) }
    : horizontal && vertical
      ? {
          height: Math.round(Math.abs(vertical.end - vertical.start)),
          width: Math.round(Math.abs(horizontal.end - horizontal.start)),
        }
      : undefined;
  if (!dimensions && !hex) return null;
  const size = dimensions
    ? `${String(dimensions.width)} × ${String(dimensions.height)} px`
    : undefined;
  const colour = copied ? "Copied" : hex?.toUpperCase();
  const widest = Math.max(size?.length ?? 0, colour?.length ?? 0) * 7 + 32;
  return (
    <span
      className="pointer-events-none absolute flex flex-col gap-0.5 whitespace-nowrap rounded-sm bg-content-fg px-2 py-1 text-xs font-semibold text-content shadow-md"
      style={cursorLabelPosition(cursor, widest)}
    >
      {size ? <span className="tabular-nums">{size}</span> : null}
      {colour ? (
        <span className="text-xxs flex items-center gap-1 text-content/80">
          {/* Inset ring keeps swatches close to the chip's own tone readable. */}
          <span
            className="size-2.5 rounded-xs ring-1 ring-content/30 ring-inset"
            style={{ backgroundColor: hex }}
          />
          <span className="tabular-nums">{colour}</span>
        </span>
      ) : null}
    </span>
  );
}

export function ToleranceIndicator({
  cursor,
  tolerance,
}: {
  cursor: Point;
  tolerance: RulerTolerance;
}) {
  return (
    <span
      className="pointer-events-none absolute min-w-30 whitespace-nowrap rounded-sm bg-content-fg px-2 py-1 text-center text-xs font-semibold text-content shadow-md"
      style={cursorLabelPosition(cursor, 120)}
    >
      Tolerance: {tolerance[0].toUpperCase() + tolerance.slice(1)}
    </span>
  );
}
