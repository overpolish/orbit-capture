import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import {
  openPermissionSettings,
  requestPermission,
} from "../../permissions/api";
import { usePermissionStore } from "../../permissions/store";
import { PermissionKind, PermissionStatus } from "../../permissions/types";
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

const synchronizeRecordingUi = async (
  mode = useRecordingSourceStore.getState().recordingMode,
  monitor = useRecordingSourceStore.getState().selectedMonitor,
) => {
  const hasSourceSelector = !["audio", "camera"].includes(mode);

  await setRecordingSourceSelectorVisible(hasSourceSelector);
  if (mode === "region" && monitor) {
    await showRegionSelector(monitor);
  } else {
    await hideRegionSelector();
  }
};

const grantPermission = (
  permission: PermissionKind,
  status: PermissionStatus,
) => {
  const action = status.canRequest
    ? requestPermission(permission)
    : openPermissionSettings(permission);
  void action;
};

export function RecordingBarWindow() {
  const { hydrated, permissions } = usePermissionStore((state) => state);
  const { recordingMode, selectedMonitor, setRecordingMode, setRegionEditing } =
    useRecordingSourceStore((state) => state);

  useEffect(() => {
    setRegionEditing(false);
  }, [setRegionEditing]);

  useEffect(() => {
    void synchronizeRecordingUi();
  }, [recordingMode, selectedMonitor]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    void listen("recording-ui://shown", () => {
      void synchronizeRecordingUi();
    }).then((listener) => {
      if (disposed) {
        listener();
      } else {
        unlisten = listener;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <RecordingBar
      initialMode={recordingMode}
      isCameraLocked={!hydrated || !permissions.camera.granted}
      isLocked={
        hydrated &&
        (!permissions.accessibility.granted ||
          !permissions.screenRecording.granted)
      }
      isMicrophoneLocked={!hydrated || !permissions.microphone.granted}
      mode={recordingMode}
      onCameraLockedPress={() => {
        grantPermission("camera", permissions.camera);
      }}
      onCancel={() => {
        void hideRecordingUi();
      }}
      onInteract={() => {
        void collapseRecordingSourceSelector();
      }}
      onMicrophoneLockedPress={() => {
        grantPermission("microphone", permissions.microphone);
      }}
      onModeChange={(mode) => {
        setRecordingMode(mode);
        void collapseRecordingSourceSelector().then(() =>
          synchronizeRecordingUi(mode, selectedMonitor),
        );
      }}
      onPointerUp={() => {
        void finishRecordingBarDrag();
      }}
    />
  );
}
