// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { Bounds } from "./pixel-analysis";
import { Measurement } from "./ruler-types";

const DURATION_MS = 180;
/** easeOutBack tension: just enough to read as a settle, not a bounce. */
const OVERSHOOT = 1.15;
/** Below this the snap is invisible, so there is nothing worth animating. */
const VISIBLE_PX = 1;

const EMPTY: ReadonlyMap<number, Bounds> = new Map();

type Settle = { from: Bounds; start: number; to: Bounds };

const easeOutBack = (ratio: number) => {
  const rest = ratio - 1;
  return 1 + (OVERSHOOT + 1) * rest ** 3 + OVERSHOOT * rest ** 2;
};

const reducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const rect = (bounds: Bounds): Bounds => ({
  height: bounds.height,
  width: bounds.width,
  x: bounds.x,
  y: bounds.y,
});

const mix = (from: Bounds, to: Bounds, ratio: number): Bounds => ({
  height: from.height + (to.height - from.height) * ratio,
  width: from.width + (to.width - from.width) * ratio,
  x: from.x + (to.x - from.x) * ratio,
  y: from.y + (to.y - from.y) * ratio,
});

/** Whether a snap moved any edge far enough for the settle to be perceptible. */
export const settleWorthwhile = (from: Bounds, to: Bounds) =>
  Math.abs(from.x - to.x) >= VISIBLE_PX ||
  Math.abs(from.y - to.y) >= VISIBLE_PX ||
  Math.abs(from.x + from.width - (to.x + to.width)) >= VISIBLE_PX ||
  Math.abs(from.y + from.height - (to.y + to.height)) >= VISIBLE_PX;

/** Advances every running settle, dropping the ones that have landed. */
const sample = (active: Map<number, Settle>, now: number) => {
  const frames = new Map<number, Bounds>();
  for (const [id, settle] of active) {
    const ratio = Math.min(1, (now - settle.start) / DURATION_MS);
    if (ratio >= 1) {
      active.delete(id);
      continue;
    }
    frames.set(id, mix(settle.from, settle.to, easeOutBack(ratio)));
  }
  return frames.size === 0 ? EMPTY : frames;
};

/**
 * Presentational-only settle: a freshly committed measurement eases from the
 * raw drag rect to its snapped bounds. Returns the interpolated bounds keyed by
 * measurement id — absent once landed, so callers fall back to the final bounds
 * and no derived logic ever sees a half-way value.
 */
export function useSettleAnimation(measurements: readonly Measurement[]) {
  const [frames, setFrames] = useState(EMPTY);
  const activeRef = useRef(new Map<number, Settle>());
  const seenRef = useRef(new Set<number>());
  const frameRef = useRef(0);

  useLayoutEffect(() => {
    const active = activeRef.current;
    const live = new Set(measurements.map((measurement) => measurement.id));
    for (const id of active.keys()) if (!live.has(id)) active.delete(id);
    for (const measurement of measurements) {
      if (seenRef.current.has(measurement.id)) continue;
      seenRef.current.add(measurement.id);
      if (!measurement.from || reducedMotion()) continue;
      active.set(measurement.id, {
        from: measurement.from,
        start: performance.now(),
        to: rect(measurement),
      });
    }
    if (active.size === 0) return;
    // Seed before paint so the box is never shown at the snapped bounds first.
    setFrames(sample(active, performance.now()));
    if (frameRef.current !== 0) return;
    const step = (now: number) => {
      setFrames(sample(active, now));
      frameRef.current = active.size === 0 ? 0 : requestAnimationFrame(step);
    };
    frameRef.current = requestAnimationFrame(step);
  }, [measurements]);

  useEffect(
    () => () => {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
    },
    [],
  );

  return frames;
}
