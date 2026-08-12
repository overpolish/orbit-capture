// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RecordingFps } from "../recording-inputs/types";
import { RecordingMode, Region } from "../recording-sources/types";

export type RecordingStatus =
  "idle" | "paused" | "recording" | "starting" | "stopping";

/**
 * Timestamps are stamped by Rust in epoch milliseconds so that a window which
 * reloads, or joins late, derives exactly the same elapsed time as every other.
 */
export type RecordingSnapshot = {
  accumulatedMs: number;
  countdownSecondsRemaining: number;
  mode: RecordingMode | null;
  pausedAtMs: number | null;
  startedAtMs: number | null;
  status: RecordingStatus;
};

export const initialRecordingSnapshot: RecordingSnapshot = {
  accumulatedMs: 0,
  countdownSecondsRemaining: 0,
  mode: null,
  pausedAtMs: null,
  startedAtMs: null,
  status: "idle",
};

export type StartRecordingOptions = {
  cameraFlipped: boolean;
  fps: RecordingFps;
  mode: RecordingMode;
  showCursor: boolean;
  systemAudio: boolean;
  systemAudioApplicationIds: string[];
  systemAudioProcessIds: number[];
  cameraFps?: number | null;
  cameraHeight?: number | null;
  cameraId?: string | null;
  cameraWidth?: number | null;
  microphoneId?: string | null;
  monitorId?: number | null;
  region?: Region | null;
  windowId?: number | null;
};

/** What the screenshot button is currently reflecting. */
export type ScreenshotState = "done" | "failed" | "idle" | "pending";

type RecordingErrorPhase = "start" | "pause" | "resume" | "stop";

export type RecordingError = {
  message: string;
  phase: RecordingErrorPhase;
};
