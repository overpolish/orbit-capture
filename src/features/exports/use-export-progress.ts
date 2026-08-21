// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

const EXPORT_PROGRESS_EVENT = "export://progress";

// The rate is measured across a trailing window rather than smoothed per event:
// the endpoints of a ten-second span average out frame-to-frame encoder jitter
// on their own, while still tracking a real change in throughput within a few
// seconds. Ten seconds is long enough to swallow a stalled disk write or a
// burst of cheap frames, short enough that the estimate is not describing a
// part of the export that is already over.
const ETA_WINDOW_MS = 10_000;
// Do not surface an estimate until the window actually spans this much
// wall-clock time, so the first estimate isn't extrapolated from a noisy burst.
// This single gate replaces the old per-event interval and sample-count gates —
// with an endpoint-based rate, closely spaced events are harmless.
const ETA_MIN_SPAN_MS = 3_000;
// Progress events arrive per encoded frame, so store at most one sample per
// this many milliseconds. The window is bounded either way; this just keeps the
// array short.
const ETA_SAMPLE_MIN_GAP_MS = 100;
// The shown estimate counts down freely but may only step back up once a
// slowdown has held for this long. A transient dip in throughput therefore
// never ticks the label upward, while a genuinely heavier stretch of the export
// is reflected after a few seconds.
const ETA_SLOWDOWN_SUSTAIN_MS = 5_000;
// A step up is only considered at all once the raw estimate exceeds the shown
// one by this margin: thirty seconds, or a quarter of the shown estimate for
// long exports, where the absolute wobble scales with what is left to encode.
// A fixed absolute margin re-triggered constantly on hour-long exports.
const ETA_SLOWDOWN_MIN_MARGIN_S = 30;
const ETA_SLOWDOWN_MARGIN_RATIO = 0.25;

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
  const samplesRef = useRef<EtaSample[]>([]);
  const displayedEtaRef = useRef<number | null>(null);
  const slowdownSinceRef = useRef<number | null>(null);

  const resetEta = useCallback(() => {
    samplesRef.current = [];
    displayedEtaRef.current = null;
    slowdownSinceRef.current = null;
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
      const samples = samplesRef.current;
      const last = samples.length > 0 ? samples[samples.length - 1] : null;
      if (last !== null && last.phase !== payload.phase) {
        // Phase boundary (recording→camera→finalizing): a span straddling it is
        // meaningless because the backend restarts its progress weighting, so
        // start a fresh window. The shown estimate is deliberately left alone
        // and simply holds until the new window is wide enough to measure.
        samples.length = 0;
      } else if (last === null || now - last.time >= ETA_SAMPLE_MIN_GAP_MS) {
        samples.push({ phase: payload.phase, progress: measured, time: now });
        // Drop samples that have fallen out of the trailing window, but keep the
        // newest of them as the anchor: pruning to the window boundary alone
        // would shrink the measured span every time, and the rate would be read
        // off a span far shorter than ETA_WINDOW_MS.
        const cutoff = now - ETA_WINDOW_MS;
        let anchor = 0;
        while (anchor + 1 < samples.length && samples[anchor + 1].time < cutoff)
          anchor += 1;
        if (anchor > 0) samples.splice(0, anchor);
      }

      let raw: number | null = null;
      if (samples.length > 1) {
        const anchorSample = samples[0];
        const newestSample = samples[samples.length - 1];
        const spanMs = newestSample.time - anchorSample.time;
        const dProgress = newestSample.progress - anchorSample.progress;
        if (spanMs >= ETA_MIN_SPAN_MS && dProgress > 0) {
          const rate = (dProgress / spanMs) * 1000; // percent per second
          raw = (100 - measured) / rate;
        }
      }

      if (raw !== null) {
        const prev = displayedEtaRef.current;
        if (prev === null || raw <= prev) {
          // Count down freely.
          displayedEtaRef.current = raw;
          slowdownSinceRef.current = null;
          setEtaSeconds(raw);
        } else {
          const margin = Math.max(
            ETA_SLOWDOWN_MIN_MARGIN_S,
            prev * ETA_SLOWDOWN_MARGIN_RATIO,
          );
          if (raw > prev + margin) {
            // Only step up once the slowdown has persisted; a momentary spike
            // resets the timer below and never reaches the label.
            slowdownSinceRef.current ??= now;
            if (now - slowdownSinceRef.current >= ETA_SLOWDOWN_SUSTAIN_MS) {
              displayedEtaRef.current = raw;
              slowdownSinceRef.current = null;
              setEtaSeconds(raw);
            }
          } else {
            slowdownSinceRef.current = null;
          }
        }
      }
      // When the rate is not measurable — warm-up, just after a phase change, or
      // no forward progress — the last shown estimate simply holds. Blanking the
      // label and bringing it back is far more distracting than a value that has
      // gone a few seconds stale, and formatEta is coarse enough to hide it.
      // Only resetEta (begin/complete/reset) clears the estimate.
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
