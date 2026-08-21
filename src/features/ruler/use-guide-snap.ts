// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef } from "react";

import { Axis, GradientField } from "./gradient-field";
import { snapGuide, SnappedGuide } from "./guide-snap";
import { Point, screenToPixel } from "./pixel-analysis";
import { rulerViewportSize } from "./ruler-viewport-size";

/**
 * Wraps {@link snapGuide} with the per-axis memory its hysteresis needs. The
 * held ridge lives in a ref because guide previews are computed while
 * rendering, one axis at a time.
 */
export function useGuideSnap({
  field,
  threshold,
  zoom,
}: {
  threshold: number;
  zoom: number;
  field?: GradientField;
}) {
  const heldRef = useRef<Partial<Record<Axis, SnappedGuide>>>({});
  return useCallback(
    (axis: Axis, point: Point) => {
      const fallback = axis === "x" ? point.x : point.y;
      if (!field) return fallback;
      const size = rulerViewportSize();
      const snapped = snapGuide(field, {
        axis,
        point: screenToPixel(point, field, size),
        previous: heldRef.current[axis],
        threshold,
        zoom,
      });
      heldRef.current = { ...heldRef.current, [axis]: snapped };
      if (!snapped) return fallback;
      const scale =
        axis === "x" ? size.width / field.width : size.height / field.height;
      return snapped.position * scale;
    },
    [field, threshold, zoom],
  );
}
