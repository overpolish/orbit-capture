// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RecordingInputs } from "../recording-inputs/types";
import { RecordingMode } from "../recording-sources/types";

export type RecordingReadiness = {
  hasCameraWarning: boolean;
  hasMicrophoneWarning: boolean;
  hasSelectedMonitor: boolean;
  hasSelectedWindow: boolean;
  hasSystemAudioWarning: boolean;
  inputs: RecordingInputs;
  isCameraLocked: boolean;
  isMicrophoneLocked: boolean;
  isScreenLocked: boolean;
  mode: RecordingMode;
};

/**
 * Modes with a capture pipeline behind them, updated as each slice lands.
 *
 * Every recording mode now has a native capture pipeline. Offering a
 * Record button that starts nothing would be worse than a disabled one, so the
 * readiness check owns this list rather than the button.
 */
const implementedModes: RecordingMode[] = [
  "audio",
  "camera",
  "region",
  "screen",
  "window",
];

/**
 * A recording can only start when its mode has both an unlocked permission and
 * an actual source. Screen modes previously ignored both, so the Record button
 * could start a recording that had nothing to capture.
 */
export const canStartRecording = ({
  hasCameraWarning,
  hasMicrophoneWarning,
  hasSelectedMonitor,
  hasSelectedWindow,
  hasSystemAudioWarning,
  inputs,
  isCameraLocked,
  isMicrophoneLocked,
  isScreenLocked,
  mode,
}: RecordingReadiness) => {
  if (!implementedModes.includes(mode)) return false;

  switch (mode) {
    case "audio":
      return (
        (inputs.systemAudio && !hasSystemAudioWarning) ||
        (inputs.microphone && !isMicrophoneLocked && !hasMicrophoneWarning)
      );
    case "camera":
      return !isCameraLocked && !hasCameraWarning;
    case "window":
      return !isScreenLocked && hasSelectedWindow;
    default:
      return !isScreenLocked && hasSelectedMonitor;
  }
};
