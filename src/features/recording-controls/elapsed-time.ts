import type { RecordingSnapshot } from "./types";

/**
 * Elapsed time is derived from the snapshot's timestamps rather than
 * accumulated tick by tick, so a window that reloads, or opens mid-recording,
 * still shows the same time as every other one.
 */
export const elapsedMilliseconds = (
  snapshot: RecordingSnapshot,
  now: number,
) => {
  const openSpan =
    snapshot.status === "recording" && snapshot.startedAtMs !== null
      ? Math.max(0, now - snapshot.startedAtMs)
      : 0;

  return snapshot.accumulatedMs + openSpan;
};

/**
 * Always hours, minutes and seconds, each zero-padded to two digits. A field
 * that only appeared once the recording passed an hour would resize the pill
 * mid-recording, so the hours are simply always there.
 */
export const formatElapsedTime = (elapsedMs: number) => {
  const totalSeconds = Math.max(0, Math.floor(elapsedMs / 1000));
  const pad = (value: number) => value.toString().padStart(2, "0");

  return {
    hours: pad(Math.floor(totalSeconds / 3600)),
    minutes: pad(Math.floor(totalSeconds / 60) % 60),
    seconds: pad(totalSeconds % 60),
  };
};
