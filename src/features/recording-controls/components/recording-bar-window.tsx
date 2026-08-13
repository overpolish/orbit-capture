// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

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
import { captureStill } from "../../screenshots/api";
import { ShortcutAction } from "../../settings/types";
import { startRecording } from "../api";
import { screenshotTarget, startRecordingOptions } from "../recording-request";
import { selectStatus, useRecordingStore } from "../store";
import { RecordingError, ScreenshotState } from "../types";

import { RecordingBar } from "./recording-bar";

const RECORDING_ERROR_EVENT = "recording://error";
const SHORTCUT_ACTION_EVENT = "global-shortcut://action";
/** How long the screenshot button holds its outcome before going back to idle. */
const SCREENSHOT_FEEDBACK_MS = 2000;

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
    setScreenshotCapture,
  } = useRecordingSourceStore((state) => state);
  const {
    fps,
    inputs,
    screenshotDestination,
    setFps,
    setInput,
    setScreenshotDestination,
  } = useRecordingInputStore((state) => state);

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

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <RecordingBar
      fps={fps}
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
          destination: screenshotDestination,
          showCursor: inputs.showCursor,
          target,
        })
          .then(() => {
            // With the clipboard off the export window opens instead, and its
            // appearance is the feedback; a check would claim a file exists.
            setScreenshotState(
              screenshotDestination === "clipboard" ? "done" : "idle",
            );
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
      onScreenshotToClipboardChange={(toClipboard) => {
        setScreenshotDestination(toClipboard ? "clipboard" : "export");
      }}
      screenshotState={screenshotState}
      screenshotToClipboard={screenshotDestination !== "export"}
      status={status}
    />
  );
}
