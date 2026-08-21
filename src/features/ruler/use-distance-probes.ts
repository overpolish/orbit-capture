// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useRef, useState } from "react";

import { Axis, GradientField } from "./gradient-field";
import {
  distanceProbeAt,
  DistanceProbe,
  PixelSize,
  Point,
} from "./pixel-analysis";
import { clipProbe } from "./probe-stops";
import { Guide, Measurement } from "./ruler-types";

export type PersistedDistanceProbe = DistanceProbe & { id: number };

/** Committed lines the probes should also stop at; only set while Alt is held. */
export type ProbeArtifacts = {
  guides: readonly Guide[];
  measurements: readonly Measurement[];
};

export function useDistanceProbes({
  artifacts,
  cursor,
  field,
  threshold,
  viewport,
}: {
  threshold: number;
  viewport: PixelSize;
  artifacts?: ProbeArtifacts;
  cursor?: Point;
  field?: GradientField;
}) {
  const [probes, setProbes] = useState<PersistedDistanceProbe[]>([]);
  const nextIdRef = useRef(1);
  const { height: viewportHeight, width: viewportWidth } = viewport;
  // Stamping reads the modifier through a ref: making `persist` depend on the
  // (per-render) artifacts object would re-register every hotkey listener.
  const artifactsRef = useRef(artifacts);
  useEffect(() => {
    artifactsRef.current = artifacts;
  }, [artifacts]);
  const previews =
    cursor && field
      ? (["x", "y"] as const).map((axis) => {
          const probe = distanceProbeAt({
            axis,
            field,
            point: cursor,
            threshold,
            viewport,
          });
          return artifacts ? clipProbe({ ...artifacts, cursor, probe }) : probe;
        })
      : [];
  const persist = useCallback(
    (axis: Axis, point: Point) => {
      if (!field) return;
      const raw = distanceProbeAt({
        axis,
        field,
        point,
        threshold,
        viewport: { height: viewportHeight, width: viewportWidth },
      });
      const held = artifactsRef.current;
      const probe = held
        ? clipProbe({ ...held, cursor: point, probe: raw })
        : raw;
      setProbes((current) => [
        ...current,
        { ...probe, id: nextIdRef.current++ },
      ]);
    },
    [field, threshold, viewportHeight, viewportWidth],
  );
  const clear = useCallback(() => {
    setProbes([]);
  }, []);
  const remove = useCallback((id: number) => {
    setProbes((current) => current.filter((probe) => probe.id !== id));
  }, []);
  /** Undo/redo puts a whole list back; the id counter only ever climbs. */
  const restore = useCallback((next: PersistedDistanceProbe[]) => {
    setProbes(next);
  }, []);
  return { clear, persist, previews, probes, remove, restore };
}
