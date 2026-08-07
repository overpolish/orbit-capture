import { usePermissionStore } from "../../permissions/store";
import {
  collapseRecordingSourceSelector,
  finishRecordingBarDrag,
  hideRecordingUi,
} from "../../recording-sources/api";

import { RecordingBar } from "./recording-bar";

export function RecordingBarWindow() {
  const { hydrated, permissions } = usePermissionStore((state) => state);

  return (
    <RecordingBar
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
      onPointerUp={() => {
        void finishRecordingBarDrag();
      }}
    />
  );
}
