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

/**
 * The cadences asked of a camera, best first. Under 50 Hz (PAL) lighting 30/60
 * flicker, so PAL asks for 50/25 — and a camera that cannot reach 50 must fall
 * back to 25 rather than to the nearer-but-flickering 30. Outside PAL the
 * recording fps leads, with its half as the fallback for 30 fps-only cameras.
 *
 * A camera that offers no PAL cadence at all (Media Foundation only lists the
 * discrete rates a webcam advertises, typically without 25 or 50) takes the
 * standard rates last rather than whatever happens to sit nearest 25, such as
 * 24; on Windows the camera's own power line frequency control handles the
 * flicker there.
 */
export const cameraRequestFps = (fps: RecordingFps, pal: boolean): number[] =>
  pal
    ? fps === 60
      ? [50, 25, 60, 30]
      : [25, 30]
    : fps === 60
      ? [60, 30]
      : [30];

export type RecordingInputs = {
  camera: boolean;
  microphone: boolean;
  showCursor: boolean;
  systemAudio: boolean;
};
