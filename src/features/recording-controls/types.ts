import { RecordingFps } from "../recording-inputs/types";
import { RecordingMode, Region } from "../recording-sources/types";

export const recordingStatuses = [
  "idle",
  "starting",
  "recording",
  "paused",
  "stopping",
] as const;

export type RecordingStatus = (typeof recordingStatuses)[number];

/**
 * Timestamps are stamped by Rust in epoch milliseconds so that a window which
 * reloads, or joins late, derives exactly the same elapsed time as every other.
 */
export type RecordingSnapshot = {
  accumulatedMs: number;
  mode: RecordingMode | null;
  pausedAtMs: number | null;
  startedAtMs: number | null;
  status: RecordingStatus;
};

export const initialRecordingSnapshot: RecordingSnapshot = {
  accumulatedMs: 0,
  mode: null,
  pausedAtMs: null,
  startedAtMs: null,
  status: "idle",
};

export type StartRecordingOptions = {
  fps: RecordingFps;
  mode: RecordingMode;
  showCursor: boolean;
  systemAudio: boolean;
  systemAudioApplicationIds: string[];
  systemAudioProcessIds: number[];
  cameraId?: string | null;
  microphoneId?: string | null;
  monitorId?: number | null;
  region?: Region | null;
  windowId?: number | null;
};

/** What the screenshot button is currently reflecting. */
export type ScreenshotState = "done" | "failed" | "idle" | "pending";

export type RecordingErrorPhase = "start" | "pause" | "resume" | "stop";

export type RecordingError = {
  message: string;
  phase: RecordingErrorPhase;
};
