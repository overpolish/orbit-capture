// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RecordingInputs } from "../recording-inputs/types";
import { RecordingMode } from "../recording-sources/types";

export type RecordingReadiness = {
  hasSelectedMonitor: boolean;
  hasSelectedWindow: boolean;
  inputs: RecordingInputs;
  isCameraLocked: boolean;
  isMicrophoneLocked: boolean;
  isScreenLocked: boolean;
  mode: RecordingMode;
};

/**
 * Modes with a capture pipeline behind them, updated as each slice lands.
 *
 * Today: screen, region and window. Still to come: camera and audio. Offering a
 * Record button that starts nothing would be worse than a disabled one, so the
 * readiness check owns this list rather than the button.
 */
const implementedModes: RecordingMode[] = ["region", "screen", "window"];

/**
 * A recording can only start when its mode has both an unlocked permission and
 * an actual source. Screen modes previously ignored both, so the Record button
 * could start a recording that had nothing to capture.
 */
export const canStartRecording = ({
  hasSelectedMonitor,
  hasSelectedWindow,
  inputs,
  isCameraLocked,
  isMicrophoneLocked,
  isScreenLocked,
  mode,
}: RecordingReadiness) => {
  if (!implementedModes.includes(mode)) return false;

  switch (mode) {
    case "audio":
      return inputs.systemAudio || (inputs.microphone && !isMicrophoneLocked);
    case "camera":
      return inputs.camera && !isCameraLocked;
    case "window":
      return !isScreenLocked && hasSelectedWindow;
    default:
      return !isScreenLocked && hasSelectedMonitor;
  }
};
