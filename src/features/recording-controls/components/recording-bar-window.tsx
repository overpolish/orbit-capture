import { getCurrentWindow } from "@tauri-apps/api/window";

import { usePermissionStore } from "../../permissions/store";

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
        void getCurrentWindow().hide();
      }}
    />
  );
}
