// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Dispatch,
  SetStateAction,
  useCallback,
  useEffect,
  useMemo,
  useRef,
} from "react";

import { Point } from "./pixel-analysis";
import { Guide, Measurement } from "./ruler-types";
import { PersistedDistanceProbe } from "./use-distance-probes";

/** Snapshots are cheap (shared slice references), so the cap is generous. */
const MAX_ENTRIES = 100;

/** The four user-mutable slices of the ruler document. */
type Snapshot = {
  guides: Guide[];
  measurements: Measurement[];
  offsets: Record<string, Point>;
  probes: PersistedDistanceProbe[];
};

const unchanged = (a: Snapshot, b: Snapshot) =>
  a.guides === b.guides &&
  a.measurements === b.measurements &&
  a.offsets === b.offsets &&
  a.probes === b.probes;

const push = (stack: Snapshot[], entry: Snapshot) => {
  stack.push(entry);
  if (stack.length > MAX_ENTRIES) stack.shift();
};

/**
 * A stable `record` the mutating hooks can take before the history hook — which
 * needs their state as its snapshot — has been created. `fill` hands the real
 * implementation over once it exists.
 */
export function useRecordSlot() {
  const recordRef = useRef<() => void>(() => undefined);
  return useMemo(
    () => ({
      fill: (record: () => void) => {
        recordRef.current = record;
      },
      record: () => {
        recordRef.current();
      },
    }),
    [],
  );
}

/**
 * Undo/redo as whole-document snapshots. Callers `record()` synchronously right
 * before a mutation; restoring pushes the slices back through their setters, so
 * ids survive and the settle animation (once per id) never replays.
 */
export function useRulerHistory({
  fill,
  guides,
  labels,
  measurements,
  probes,
  setMeasurements,
}: {
  fill: (record: () => void) => void;
  guides: { guides: Guide[]; restore: (guides: Guide[]) => void };
  labels: {
    offsets: Record<string, Point>;
    restore: (offsets: Record<string, Point>) => void;
  };
  measurements: Measurement[];
  probes: {
    probes: PersistedDistanceProbe[];
    restore: (probes: PersistedDistanceProbe[]) => void;
  };
  setMeasurements: Dispatch<SetStateAction<Measurement[]>>;
}) {
  const undoRef = useRef<Snapshot[]>([]);
  const redoRef = useRef<Snapshot[]>([]);
  const currentRef = useRef<Snapshot>({
    guides: guides.guides,
    measurements,
    offsets: labels.offsets,
    probes: probes.probes,
  });
  const targetsRef = useRef({ guides, labels, probes, setMeasurements });

  useEffect(() => {
    currentRef.current = {
      guides: guides.guides,
      measurements,
      offsets: labels.offsets,
      probes: probes.probes,
    };
    targetsRef.current = { guides, labels, probes, setMeasurements };
  }, [guides, labels, measurements, probes, setMeasurements]);

  const record = useCallback(() => {
    const stack = undoRef.current;
    const top = stack[stack.length - 1] as Snapshot | undefined;
    // A no-op record (a label click without a drag, say) must not eat the redo
    // stack, so the dedupe returns before anything is discarded.
    if (top && unchanged(top, currentRef.current)) return;
    push(stack, currentRef.current);
    redoRef.current = [];
  }, []);

  useEffect(() => {
    fill(record);
  }, [fill, record]);

  const swap = useCallback((from: Snapshot[], to: Snapshot[]) => {
    const entry = from.pop();
    if (!entry) return false;
    push(to, currentRef.current);
    const targets = targetsRef.current;
    targets.guides.restore(entry.guides);
    targets.labels.restore(entry.offsets);
    targets.probes.restore(entry.probes);
    targets.setMeasurements(entry.measurements);
    // Repeated presses land before the next render refreshes the ref.
    currentRef.current = entry;
    return true;
  }, []);

  const undo = useCallback(
    () => swap(undoRef.current, redoRef.current),
    [swap],
  );
  const redo = useCallback(
    () => swap(redoRef.current, undoRef.current),
    [swap],
  );

  return { redo, undo };
}
