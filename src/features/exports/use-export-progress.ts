// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

const EXPORT_PROGRESS_EVENT = "export://progress";

// Weight applied to each fresh instantaneous rate in the exponential moving
// average. Low enough that a single unusually fast or slow event only nudges
// the smoothed rate rather than swinging it, so the shown estimate is steady.
const ETA_EMA_ALPHA = 0.3;
// Ignore events closer together than this — a tiny delta over a tiny interval
// produces a wildly noisy instantaneous rate.
const ETA_MIN_INTERVAL_MS = 50;
// Do not surface an estimate until the rate has been smoothed across at least
// this many consecutive same-phase intervals.
const ETA_MIN_RATE_SAMPLES = 2;
// …and until this much wall-clock time has elapsed since the first measured
// event, so the very first estimate isn't extrapolated from a noisy burst.
const ETA_MIN_SPAN_MS = 1500;
// The shown estimate only ever counts down, so frame-to-frame rate wobble
// (which pushes the raw estimate up and down) can never flicker the label. The
// one exception is a genuine slowdown: if the raw estimate jumps up by more
// than this many seconds — far beyond normal wobble — the export really did
// slow (e.g. a heavier phase), so the countdown is allowed to step back up.
const ETA_SLOWDOWN_ESCAPE_S = 45;

type EtaSample = { phase: ExportPhase; progress: number; time: number };

export type ExportPhase = "camera" | "finalizing" | "recording";

type ExportProgressEvent = {
  artifactId: number;
  phase: ExportPhase;
  progressPercent: number;
};

export function useExportProgress(artifactId?: number) {
  const [phase, setPhase] = useState<ExportPhase>("recording");
  const [progress, setProgress] = useState<number | null>(null);
  const [etaSeconds, setEtaSeconds] = useState<number | null>(null);

  // Wall-clock timing state for the ETA. Kept in refs so it survives renders
  // and never itself triggers one; only the derived `etaSeconds` is state.
  const startTimeRef = useRef<number | null>(null);
  const lastSampleRef = useRef<EtaSample | null>(null);
  const smoothedRateRef = useRef<number | null>(null);
  const rateSampleCountRef = useRef(0);
  const displayedEtaRef = useRef<number | null>(null);

  const resetEta = useCallback(() => {
    startTimeRef.current = null;
    lastSampleRef.current = null;
    smoothedRateRef.current = null;
    rateSampleCountRef.current = 0;
    displayedEtaRef.current = null;
    setEtaSeconds(null);
  }, []);

  useEffect(() => {
    if (artifactId === undefined) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ExportProgressEvent>(EXPORT_PROGRESS_EVENT, ({ payload }) => {
      if (disposed || payload.artifactId !== artifactId) return;
      setPhase(payload.phase);
      // The backend weights screen and camera work and reserves the final one
      // percent for validating that both atomic renames have published.
      const measured = Math.min(99, payload.progressPercent);
      setProgress(measured);

      const now = Date.now();
      startTimeRef.current ??= now;
      const last = lastSampleRef.current;
      const sample: EtaSample = {
        phase: payload.phase,
        progress: measured,
        time: now,
      };
      if (last === null || payload.phase !== last.phase) {
        // First sample, or a phase boundary (recording→camera→finalizing):
        // (re)start the baseline. A delta straddling a phase change is
        // meaningless, and its progress weighting restarts anyway.
        lastSampleRef.current = sample;
      } else {
        const dtMs = now - last.time;
        const dProgress = measured - last.progress;
        if (dtMs >= ETA_MIN_INTERVAL_MS && dProgress > 0) {
          const instantRate = (dProgress / dtMs) * 1000; // percent per second
          smoothedRateRef.current =
            smoothedRateRef.current === null
              ? instantRate
              : ETA_EMA_ALPHA * instantRate +
                (1 - ETA_EMA_ALPHA) * smoothedRateRef.current;
          rateSampleCountRef.current += 1;
          // Advance the baseline only once a sample is actually taken. Progress
          // events arrive per encoded frame — faster than ETA_MIN_INTERVAL_MS —
          // so resetting it every event would leave every interval below the
          // threshold and no rate would ever be measured.
          lastSampleRef.current = sample;
        }
        // Otherwise keep the baseline so the interval keeps growing toward the
        // threshold instead of being discarded.
      }

      const rate = smoothedRateRef.current;
      const spanMs = now - startTimeRef.current;
      const raw =
        rate !== null &&
        rate > 0 &&
        rateSampleCountRef.current >= ETA_MIN_RATE_SAMPLES &&
        spanMs >= ETA_MIN_SPAN_MS
          ? (100 - measured) / rate
          : null;
      if (raw === null) {
        displayedEtaRef.current = null;
        setEtaSeconds(null);
      } else {
        const prev = displayedEtaRef.current;
        // Count down monotonically: take the lower of the last shown estimate
        // and the new one, so wobble never ticks the label upward. Only a jump
        // larger than normal wobble (a real slowdown) is allowed to raise it.
        const next =
          prev === null || raw - prev > ETA_SLOWDOWN_ESCAPE_S
            ? raw
            : Math.min(prev, raw);
        displayedEtaRef.current = next;
        setEtaSeconds(next);
      }
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [artifactId]);

  const begin = useCallback(
    (hasMeasuredProgress: boolean) => {
      setPhase("recording");
      setProgress(hasMeasuredProgress ? 0 : null);
      resetEta();
    },
    [resetEta],
  );
  const complete = useCallback(() => {
    setProgress(100);
    resetEta();
  }, [resetEta]);
  const reset = useCallback(() => {
    setPhase("recording");
    setProgress(null);
    resetEta();
  }, [resetEta]);

  return { begin, complete, etaSeconds, phase, progress, reset };
}
