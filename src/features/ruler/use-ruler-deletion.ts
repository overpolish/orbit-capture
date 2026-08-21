// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Dispatch, SetStateAction, useCallback, useRef } from "react";

import { Point } from "./pixel-analysis";
import { DistanceProbe, Guide, Measurement } from "./ruler-types";

/** Cursor-to-line distance, in screen px, that counts as "selected". */
const SELECT_RADIUS = 6;

/** Trailing ids in a label key: `m3`, `p7`, `gx:2-5`. */
const labelIds = (key: string) =>
  key
    .slice(1)
    .split(/[:-]/u)
    .map(Number)
    .filter((value) => Number.isFinite(value));

export type SelectedLine = {
  id: number;
  kind: "guide" | "measurement" | "probe";
};

/** The line a hovered label chip stands for — what deleting it would remove. */
export const selectedFromLabel = (key: string): SelectedLine => {
  const ids = labelIds(key);
  if (key.startsWith("m")) return { id: ids[0], kind: "measurement" };
  if (key.startsWith("p")) return { id: ids[0], kind: "probe" };
  // A gap chip deletes the later-placed of its two guides.
  return { id: Math.max(...ids), kind: "guide" };
};

/**
 * Everything the delete key can act on: a hovered label chip (which owns its
 * measurement, probe, or gap), or the guide/probe line nearest the cursor.
 * Call `selectLine` during render once the placement state is known — the
 * chosen line wears a pulsing halo, and the keydown handler reads the same
 * choice through a ref so the key always deletes what the halo shows.
 */
export function useRulerDeletion({
  clearHover,
  guides,
  hovered,
  measurements,
  probes,
  record,
  removeGuide,
  removeProbe,
  setMeasurements,
  zoom,
}: {
  clearHover: () => void;
  guides: readonly Guide[];
  hovered: string | undefined;
  measurements: readonly Measurement[];
  probes: readonly DistanceProbe[];
  /** Snapshots the document; only called once a deletion is certain. */
  record: () => void;
  removeGuide: (id: number) => void;
  removeProbe: (id: number) => void;
  setMeasurements: Dispatch<SetStateAction<Measurement[]>>;
  zoom: number;
}) {
  const selectedRef = useRef<SelectedLine | undefined>(undefined);

  const selectLine = ({
    active,
    cursor,
  }: {
    active: boolean;
    cursor?: Point;
  }): SelectedLine | undefined => {
    let best: SelectedLine | undefined;
    if (active && cursor) {
      let bestDistance = SELECT_RADIUS / Math.max(0.01, zoom);
      for (const guide of guides) {
        const distance = Math.abs(
          (guide.axis === "x" ? cursor.x : cursor.y) - guide.position,
        );
        if (distance <= bestDistance) {
          best = { id: guide.id, kind: "guide" };
          bestDistance = distance;
        }
      }
      for (const probe of probes) {
        if (probe.id === undefined) continue;
        // A probe measures ALONG its axis: its line runs from start to end on
        // that axis, sitting at `position` on the other one.
        const along = probe.axis === "x" ? cursor.x : cursor.y;
        const cross = probe.axis === "x" ? cursor.y : cursor.x;
        if (along < Math.min(probe.start, probe.end) - bestDistance) continue;
        if (along > Math.max(probe.start, probe.end) + bestDistance) continue;
        const distance = Math.abs(cross - probe.position);
        if (distance <= bestDistance) {
          best = { id: probe.id, kind: "probe" };
          bestDistance = distance;
        }
      }
      for (const measurement of measurements) {
        // Distance to the nearest point of the box OUTLINE (not its interior),
        // so a box only lights up when the cursor rides its border.
        const dx = Math.max(
          measurement.x - cursor.x,
          0,
          cursor.x - (measurement.x + measurement.width),
        );
        const dy = Math.max(
          measurement.y - cursor.y,
          0,
          cursor.y - (measurement.y + measurement.height),
        );
        const outside = Math.hypot(dx, dy);
        const inside = Math.min(
          cursor.x - measurement.x,
          measurement.x + measurement.width - cursor.x,
          cursor.y - measurement.y,
          measurement.y + measurement.height - cursor.y,
        );
        const distance = outside > 0 ? outside : Math.max(0, inside);
        if (distance <= bestDistance) {
          best = { id: measurement.id, kind: "measurement" };
          bestDistance = distance;
        }
      }
    }
    selectedRef.current = best;
    return best;
  };

  const deleteHovered = useCallback(() => {
    if (hovered) {
      const ids = labelIds(hovered);
      record();
      if (hovered.startsWith("m"))
        setMeasurements((current) =>
          current.filter((measurement) => measurement.id !== ids[0]),
        );
      else if (hovered.startsWith("p")) removeProbe(ids[0]);
      // Dropping the later-placed guide merges the neighbouring gaps.
      else removeGuide(Math.max(...ids));
      clearHover();
      return true;
    }
    const selected = selectedRef.current;
    if (selected === undefined) return false;
    record();
    if (selected.kind === "guide") removeGuide(selected.id);
    else if (selected.kind === "probe") removeProbe(selected.id);
    else
      setMeasurements((current) =>
        current.filter((measurement) => measurement.id !== selected.id),
      );
    return true;
  }, [clearHover, hovered, record, removeGuide, removeProbe, setMeasurements]);

  return { deleteHovered, selectLine };
}
