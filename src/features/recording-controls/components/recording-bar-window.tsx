import { usePermissionStore } from "../../permissions/store";
import {
  collapseRecordingSourceSelector,
  finishRecordingBarDrag,
  hideRecordingUi,
} from "../../recording-sources/api";
import { useRecordingSourceStore } from "../../recording-sources/store";

import { RecordingBar } from "./recording-bar";

export function RecordingBarWindow() {
  const { hydrated, permissions } = usePermissionStore((state) => state);
  const { recordingMode, setRecordingMode } = useRecordingSourceStore(
    (state) => state,
  );

  return (
    <RecordingBar
      initialMode={recordingMode}
      isCameraLocked={hydrated && !permissions.camera.granted}
      isLocked={
        hydrated &&
        (!permissions.accessibility.granted ||
          !permissions.screenRecording.granted)
      }
      isMicrophoneLocked={hydrated && !permissions.microphone.granted}
      onCancel={() => {
        void hideRecordingUi();
      }}
      onInteract={() => {
        void collapseRecordingSourceSelector();
      }}
      onModeChange={(mode) => {
        void collapseRecordingSourceSelector();
        setRecordingMode(mode);
      }}
      onPointerUp={() => {
        void finishRecordingBarDrag();
      }}
    />
  );
}
