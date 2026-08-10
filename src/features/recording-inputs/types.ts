// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type InputDevice = {
  id: string;
  label: string;
  isDefault?: boolean;
};

/** One exact native capture mode advertised by a physical camera. */
export type CameraResolution = InputDevice & {
  fps: number;
  height: number;
  width: number;
};

export type CameraDevice = InputDevice & {
  modes: CameraResolution[];
};

export type SystemAudioSource = InputDevice & {
  kind: "all" | "application";
  iconPath?: string | null;
  processIds?: number[];
};

/**
 * Frame rates the bar offers. Two, because the choice is between "smooth" and
 * "half the file size", and every value between them is a worse version of one
 * of those answers.
 */
export const recordingFpsOptions = [30, 60] as const;

export type RecordingFps = (typeof recordingFpsOptions)[number];

export type RecordingInputs = {
  camera: boolean;
  microphone: boolean;
  showCursor: boolean;
  systemAudio: boolean;
};
