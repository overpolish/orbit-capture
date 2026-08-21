// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef, useState } from "react";

import { Axis, GradientField } from "./gradient-field";
import { Point } from "./pixel-analysis";
import { Guide } from "./ruler-types";
import { useGuideSnap } from "./use-guide-snap";

/** Cross-axis coordinate a guide on `axis` is anchored to. */
const anchorOf = (axis: Axis, point: Point) =>
  axis === "x" ? point.y : point.x;

/**
 * Guide placement, snapping and removal. The held axis stays with the hotkeys
 * hook, so both the commit and the preview take it as an argument.
 */
export function useRulerGuides({
  field,
  threshold,
  zoom,
}: {
  threshold: number;
  zoom: number;
  field?: GradientField;
}) {
  const [guides, setGuides] = useState<Guide[]>([]);
  const nextIdRef = useRef(1);
  const snap = useGuideSnap({ field, threshold, zoom });

  const place = (axis: Axis, point: Point) => {
    setGuides((current) => [
      ...current,
      {
        anchor: anchorOf(axis, point),
        axis,
        id: nextIdRef.current++,
        position: snap(axis, point),
      },
    ]);
  };

  /** Carrying a placed guide: same snap path as placement, anchor included. */
  const move = (id: number, point: Point) => {
    const axis = guides.find((guide) => guide.id === id)?.axis;
    if (!axis) return;
    const anchor = anchorOf(axis, point);
    const position = snap(axis, point);
    setGuides((current) =>
      current.map((guide) =>
        guide.id === id ? { ...guide, anchor, position } : guide,
      ),
    );
  };

  const remove = useCallback((id: number) => {
    setGuides((current) => current.filter((guide) => guide.id !== id));
  }, []);

  /** Undo/redo puts a whole list back; the id counter only ever climbs. */
  const restore = useCallback((next: Guide[]) => {
    setGuides(next);
  }, []);

  const previewAt = (axis: Axis, cursor: Point): Guide => ({
    anchor: anchorOf(axis, cursor),
    axis,
    id: 0,
    position: snap(axis, cursor),
    transient: true,
  });

  return { guides, move, place, previewAt, remove, restore };
}
