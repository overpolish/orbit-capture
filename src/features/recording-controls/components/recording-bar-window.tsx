import { useEffect } from "react";

import { usePermissionStore } from "../../permissions/store";
import {
  collapseRecordingSourceSelector,
  finishRecordingBarDrag,
  hideRecordingUi,
  hideRegionSelector,
  setRecordingSourceSelectorVisible,
  showRegionSelector,
} from "../../recording-sources/api";
import { useRecordingSourceStore } from "../../recording-sources/store";

import { RecordingBar } from "./recording-bar";

export function RecordingBarWindow() {
  const { hydrated, permissions } = usePermissionStore((state) => state);
  const { recordingMode, selectedMonitor, setRecordingMode, setRegionEditing } =
    useRecordingSourceStore((state) => state);

  useEffect(() => {
    setRegionEditing(false);
  }, [setRegionEditing]);

  useEffect(() => {
    void setRecordingSourceSelectorVisible(recordingMode !== "audio");
    if (recordingMode === "region" && selectedMonitor) {
      void showRegionSelector(selectedMonitor);
    } else {
      void hideRegionSelector();
    }
  }, [recordingMode, selectedMonitor]);

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
        void setRecordingSourceSelectorVisible(mode !== "audio");
        if (mode === "region" && selectedMonitor) {
          void showRegionSelector(selectedMonitor);
        } else {
          void hideRegionSelector();
        }
      }}
      onPointerUp={() => {
        void finishRecordingBarDrag();
      }}
    />
  );
}
