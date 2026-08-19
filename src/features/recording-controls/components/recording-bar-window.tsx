// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

import { focusExportWindow } from "../../exports/api";
import {
  selectHasPendingRecording,
  selectHasPendingScreenshot,
  useExportStore,
} from "../../exports/store";
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
import { cameraRequestFps } from "../../recording-inputs/types";
import {
  collapseRecordingSourceSelector,
  finishRecordingBarDrag,
  hideRecordingUi,
  hideRegionSelector,
  listWindows,
  recordingUiVisible,
  setRecordingSourceSelectorVisible,
  showRegionSelector,
} from "../../recording-sources/api";
import { useRecordingSourceStore } from "../../recording-sources/store";
import { captureStill } from "../../screenshots/api";
import { ShortcutAction } from "../../settings/types";
import { startRecording } from "../api";
import { screenshotTarget, startRecordingOptions } from "../recording-request";
import { selectStatus, useRecordingStore } from "../store";
import { RecordingError, ScreenshotAction, ScreenshotState } from "../types";
import { useRecordingInputAvailability } from "../use-recording-input-availability";

import { RecordingBar } from "./recording-bar";

const RECORDING_ERROR_EVENT = "recording://error";
/** A recording started without selected inputs whose devices had vanished. */
const RECORDING_INPUTS_SKIPPED_EVENT = "recording://inputs-skipped";

const SKIPPED_INPUT_LABELS: Record<string, string> = {
  camera: "camera",
  microphone: "microphone",
  systemAudio: "system audio",
};
const SHORTCUT_ACTION_EVENT = "global-shortcut://action";
/** How long the screenshot button holds its outcome before going back to idle. */
const SCREENSHOT_FEEDBACK_MS = 2000;

const validateSelectedWindow = async () => {
  const selected = useRecordingSourceStore.getState().selectedWindow;
  if (!selected) return;

  try {
    const available = await listWindows();
    const { selectedWindow, setSelectedWindow } =
      useRecordingSourceStore.getState();
    if (!selectedWindow) return;
    if (
      !available.some(
        (window) =>
          window.id === selectedWindow.id && window.pid === selectedWindow.pid,
      )
    ) {
      setSelectedWindow(null);
    }
  } catch (error) {
    console.error("Could not validate the selected window", error);
  }
};

const synchronizeRecordingUi = async (
  mode = useRecordingSourceStore.getState().recordingMode,
  monitor = useRecordingSourceStore.getState().selectedMonitor,
) => {
  // A rehydrate mid-recording must not re-show the chrome that starting the
  // recording deliberately hid.
  if (useRecordingStore.getState().snapshot.status !== "idle") return;
  // Nor may one take the region overlay away from a screenshot session, which
  // borrows it regardless of the recording mode.
  if (useRecordingSourceStore.getState().isScreenshotCapture) return;

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
  const hasPendingRecording = useExportStore(selectHasPendingRecording);
  const hasPendingScreenshot = useExportStore(selectHasPendingScreenshot);
  const canRecordCamera = usePermissionStore(selectCanRecordCamera);
  const canRecordMicrophone = usePermissionStore(selectCanRecordMicrophone);
  const canRecordScreen = usePermissionStore(selectCanRecordScreen);
  const canScreenshot = usePermissionStore(selectCanScreenshot);
  const hydrated = usePermissionStore((state) => state.hydrated);
  const permissions = usePermissionStore((state) => state.permissions);
  const status = useRecordingStore(selectStatus);
  const [screenshotFeedback, setScreenshotFeedback] = useState<{
    action: ScreenshotAction;
    state: ScreenshotState;
  }>({ action: "export", state: "idle" });
  const [isOcrActive, setIsOcrActive] = useState(false);
  const [isRecordingUiVisible, setIsRecordingUiVisible] = useState(false);
  const screenshotResetRef = useRef<number | undefined>(undefined);
  const {
    isRegionEditing,
    isScreenshotCapture,
    recordingMode,
    selectedMonitor,
    selectedWindow,
    setRecordingMode,
    setScreenshotCapture,
  } = useRecordingSourceStore((state) => state);
  const {
    cameraPalById,
    fps,
    inputs,
    selectedCamera,
    selectedMicrophone,
    selectedSystemAudio,
    setFps,
    setInput,
  } = useRecordingInputStore((state) => state);
  const inputAvailability = useRecordingInputAvailability({
    active:
      isRecordingUiVisible &&
      !isOcrActive &&
      !isRegionEditing &&
      !isScreenshotCapture &&
      screenshotFeedback.state !== "pending" &&
      status === "idle",
    cameraEnabled: inputs.camera || recordingMode === "camera",
    cameraFps: cameraRequestFps(
      fps,
      selectedCamera ? (cameraPalById[selectedCamera.id] ?? false) : false,
    ),
    cameraPermissionGranted: hydrated && permissions.camera.granted,
    microphoneEnabled: inputs.microphone,
    microphonePermissionGranted: hydrated && permissions.microphone.granted,
    screenRecordingPermissionGranted:
      hydrated && permissions.screenRecording.granted,
    selectedCamera,
    selectedMicrophone,
    selectedSystemAudio,
    systemAudioEnabled: inputs.systemAudio,
  });

  useEffect(() => {
    // Editing, and any screenshot session that outlived a previous run, belong
    // to a window that is gone.
    setScreenshotCapture(false);
  }, [setScreenshotCapture]);

  useEffect(
    () => () => {
      window.clearTimeout(screenshotResetRef.current);
    },
    [],
  );

  useEffect(() => {
    // Returning to idle does not mean the controls should return: a completed
    // capture hands ownership to the export window. Explicitly showing the
    // recording UI emits the event below and synchronizes it at that point.
    void synchronizeRecordingUi();
  }, [recordingMode, selectedMonitor]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let unlistenHidden: UnlistenFn | undefined;
    let unlistenOcrEnded: UnlistenFn | undefined;
    let unlistenOcrStarted: UnlistenFn | undefined;
    let disposed = false;
    let receivedVisibilityEvent = false;

    void Promise.all([
      listen("recording-ui://shown", () => {
        receivedVisibilityEvent = true;
        setIsRecordingUiVisible(true);
        void synchronizeRecordingUi();
        void validateSelectedWindow();
      }),
      listen("recording-ui://hidden", () => {
        receivedVisibilityEvent = true;
        setIsRecordingUiVisible(false);
      }),
      listen("text-recognition://started", () => {
        setIsOcrActive(true);
      }),
      listen("text-recognition://ended", () => {
        setIsOcrActive(false);
      }),
    ]).then(([shown, hidden, ocrStarted, ocrEnded]) => {
      if (disposed) {
        shown();
        hidden();
        ocrStarted();
        ocrEnded();
      } else {
        unlisten = shown;
        unlistenHidden = hidden;
        unlistenOcrStarted = ocrStarted;
        unlistenOcrEnded = ocrEnded;
      }
      void recordingUiVisible()
        .then((visible) => {
          if (!disposed && !receivedVisibilityEvent) {
            setIsRecordingUiVisible(visible);
            if (visible) void validateSelectedWindow();
          }
        })
        .catch(() => {});
    });

    return () => {
      disposed = true;
      unlisten?.();
      unlistenHidden?.();
      unlistenOcrEnded?.();
      unlistenOcrStarted?.();
    };
  }, []);

  const takeScreenshot = (destination: ScreenshotAction) => {
    const target = screenshotTarget();
    if (!target) return;

    window.clearTimeout(screenshotResetRef.current);
    setScreenshotFeedback({ action: destination, state: "pending" });
    captureStill({
      destination,
      showCursor: inputs.showCursor,
      target,
    })
      .then(() => {
        // Opening the export window is its own success feedback. Clipboard
        // capture stays on the bar, so acknowledge it here instead.
        setScreenshotFeedback({
          action: destination,
          state: destination === "clipboard" ? "done" : "idle",
        });
      })
      .catch((error: unknown) => {
        console.error("Could not take the screenshot", error);
        setScreenshotFeedback({ action: destination, state: "failed" });
      })
      .finally(() => {
        screenshotResetRef.current = window.setTimeout(() => {
          setScreenshotFeedback({ action: destination, state: "idle" });
        }, SCREENSHOT_FEEDBACK_MS);
      });
  };

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    // Emitting to a window does not scope delivery: `listen` registers for any
    // target, so every window sees every shortcut action and each listener has
    // to match the one it owns exactly.
    void listen<ShortcutAction>(SHORTCUT_ACTION_EVENT, ({ payload }) => {
      if (payload !== "startStopRecording") return;
      startRecording(startRecordingOptions()).catch((error: unknown) => {
        console.error("Could not start the recording", error);
      });
    }).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
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

    let unlistenSkipped: (() => void) | undefined;
    // The dock already reflects the sanitized inputs (their meters are gone);
    // this names what was dropped and why the recording still started.
    void listen<{ inputs: string[] }>(
      RECORDING_INPUTS_SKIPPED_EVENT,
      ({ payload }) => {
        const labels = payload.inputs
          .map((input) => SKIPPED_INPUT_LABELS[input] ?? input)
          .join(", ");
        console.warn(
          `Recording started without ${labels}: the selected device is no longer available.`,
        );
      },
    ).then((listener) => {
      if (disposed) {
        listener();
      } else {
        unlistenSkipped = listener;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
      unlistenSkipped?.();
    };
  }, []);

  return (
    <RecordingBar
      fps={fps}
      hasCameraWarning={inputAvailability.cameraMissing}
      hasMicrophoneWarning={inputAvailability.microphoneMissing}
      hasSelectedMonitor={selectedMonitor !== null}
      hasSelectedWindow={selectedWindow !== null}
      hasSystemAudioWarning={inputAvailability.systemAudioMissing}
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
      onFocusPendingExport={() => {
        // Only a pending recording routes here now; a screenshot workspace
        // never blocks a capture.
        focusExportWindow("recording").catch((error: unknown) => {
          console.error("Could not focus the export window", error);
        });
      }}
      onFpsChange={setFps}
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
        void Promise.all([
          collapseRecordingSourceSelector(),
          hideRecordingOptions(),
        ]).then(() => synchronizeRecordingUi(mode, selectedMonitor));
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
        takeScreenshot("export");
      }}
      onScreenshotToClipboard={() => {
        takeScreenshot("clipboard");
      }}
      pendingExports={{
        recording: hasPendingRecording,
        screenshot: hasPendingScreenshot,
      }}
      screenshotAction={screenshotFeedback.action}
      screenshotState={screenshotFeedback.state}
      status={status}
    />
  );
}
