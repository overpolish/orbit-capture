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
