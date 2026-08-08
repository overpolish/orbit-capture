import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import {
  openPermissionSettings,
  requestPermission,
} from "../../permissions/api";
import {
  selectCanRecordCamera,
  selectCanRecordMicrophone,
  selectCanRecordScreen,
  selectCanScreenshot,
  usePermissionStore,
} from "../../permissions/store";
import { PermissionKind, PermissionStatus } from "../../permissions/types";
import {
  hideRecordingOptions,
  toggleRecordingOptions,
} from "../../recording-inputs/api";
import { useRecordingInputStore } from "../../recording-inputs/store";
import {
  collapseRecordingSourceSelector,
  finishRecordingBarDrag,
  hideRecordingUi,
  hideRegionSelector,
  setRecordingSourceSelectorVisible,
  showRegionSelector,
} from "../../recording-sources/api";
import { useRecordingSourceStore } from "../../recording-sources/store";
import { captureStill, ScreenshotTarget } from "../../screenshots/api";
import { startRecording } from "../api";
import { selectStatus, useRecordingStore } from "../store";
import {
  RecordingError,
  ScreenshotState,
  StartRecordingOptions,
} from "../types";

import { RecordingBar } from "./recording-bar";

const RECORDING_ERROR_EVENT = "recording://error";
/** How long the screenshot button holds its outcome before going back to idle. */
const SCREENSHOT_FEEDBACK_MS = 2000;

const synchronizeRecordingUi = async (
  mode = useRecordingSourceStore.getState().recordingMode,
  monitor = useRecordingSourceStore.getState().selectedMonitor,
) => {
  // A rehydrate mid-recording must not re-show the chrome that starting the
  // recording deliberately hid.
  if (useRecordingStore.getState().snapshot.status !== "idle") return;

  const hasSourceSelector = !["audio", "camera"].includes(mode);

  await setRecordingSourceSelectorVisible(hasSourceSelector);
  if (mode === "region" && monitor) {
    await showRegionSelector(monitor);
  } else {
    await hideRegionSelector();
  }
};

const startRecordingOptions = (): StartRecordingOptions => {
  const { recordingMode, region, selectedMonitor, selectedWindow } =
    useRecordingSourceStore.getState();
  const { inputs, selectedCamera, selectedMicrophone } =
    useRecordingInputStore.getState();
  const wantsCamera = inputs.camera || recordingMode === "camera";

  return {
    cameraId: wantsCamera ? (selectedCamera?.id ?? null) : null,
    microphoneId: inputs.microphone ? (selectedMicrophone?.id ?? null) : null,
    mode: recordingMode,
    monitorId: selectedMonitor?.id ?? null,
    region: recordingMode === "region" ? region : null,
    showCursor: inputs.showCursor,
    systemAudio: inputs.systemAudio,
    windowId: selectedWindow?.id ?? null,
  };
};

/** Mirrors how `startRecordingOptions` pairs a region with its monitor. */
const screenshotTarget = (): ScreenshotTarget | null => {
  const { recordingMode, region, selectedMonitor, selectedWindow } =
    useRecordingSourceStore.getState();

  if (recordingMode === "window") {
    return selectedWindow
      ? { kind: "window", windowId: selectedWindow.id }
      : null;
  }
  if (!selectedMonitor) return null;

  return recordingMode === "region"
    ? { kind: "region", monitorId: selectedMonitor.id, region }
    : { kind: "screen", monitorId: selectedMonitor.id };
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
  const canRecordCamera = usePermissionStore(selectCanRecordCamera);
  const canRecordMicrophone = usePermissionStore(selectCanRecordMicrophone);
  const canRecordScreen = usePermissionStore(selectCanRecordScreen);
  const canScreenshot = usePermissionStore(selectCanScreenshot);
  const hydrated = usePermissionStore((state) => state.hydrated);
  const permissions = usePermissionStore((state) => state.permissions);
  const status = useRecordingStore(selectStatus);
  const [screenshotState, setScreenshotState] =
    useState<ScreenshotState>("idle");
  const screenshotResetRef = useRef<number | undefined>(undefined);
  const {
    recordingMode,
    selectedMonitor,
    selectedWindow,
    setRecordingMode,
    setRegionEditing,
  } = useRecordingSourceStore((state) => state);
  const { inputs, screenshotToClipboard, setInput, setScreenshotToClipboard } =
    useRecordingInputStore((state) => state);

  useEffect(() => {
    setRegionEditing(false);
  }, [setRegionEditing]);

  useEffect(
    () => () => {
      window.clearTimeout(screenshotResetRef.current);
    },
    [],
  );

  useEffect(() => {
    void synchronizeRecordingUi();
  }, [recordingMode, selectedMonitor, status]);

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

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    // There is no toast surface, so a failure is logged and the UI simply
    // follows the state Rust reverted to.
    void listen<RecordingError>(RECORDING_ERROR_EVENT, ({ payload }) => {
      console.error(`Recording ${payload.phase} failed: ${payload.message}`);
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
      hasSelectedMonitor={selectedMonitor !== null}
      hasSelectedWindow={selectedWindow !== null}
      initialMode={recordingMode}
      inputs={inputs}
      isCameraLocked={!hydrated || !canRecordCamera}
      isLocked={hydrated && !canRecordScreen}
      isMicrophoneLocked={!hydrated || !canRecordMicrophone}
      isScreenshotLocked={hydrated && !canScreenshot}
      mode={recordingMode}
      onCameraLockedPress={() => {
        grantPermission("camera", permissions.camera);
      }}
      onCancel={() => {
        void hideRecordingUi();
      }}
      onInputChange={setInput}
      onInteract={() => {
        void collapseRecordingSourceSelector();
        void hideRecordingOptions();
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
      onOptions={(anchorX) => {
        void collapseRecordingSourceSelector().then(() =>
          toggleRecordingOptions(anchorX),
        );
      }}
      onPointerUp={() => {
        void finishRecordingBarDrag();
      }}
      onRecord={() => {
        startRecording(startRecordingOptions()).catch((error: unknown) => {
          console.error("Could not start the recording", error);
        });
      }}
      onScreenshot={() => {
        const target = screenshotTarget();
        if (!target) return;

        // The check has to mean the file exists. Saving encodes a few hundred
        // milliseconds of pixels, so claiming success on the press would be a
        // lie for exactly the case that takes long enough to notice.
        window.clearTimeout(screenshotResetRef.current);
        setScreenshotState("pending");
        captureStill({
          showCursor: inputs.showCursor,
          target,
          toClipboard: screenshotToClipboard,
        })
          .then(() => {
            setScreenshotState("done");
          })
          .catch((error: unknown) => {
            console.error("Could not take the screenshot", error);
            setScreenshotState("failed");
          })
          .finally(() => {
            screenshotResetRef.current = window.setTimeout(() => {
              setScreenshotState("idle");
            }, SCREENSHOT_FEEDBACK_MS);
          });
      }}
      onScreenshotToClipboardChange={setScreenshotToClipboard}
      screenshotState={screenshotState}
      screenshotToClipboard={screenshotToClipboard}
      status={status}
    />
  );
}
