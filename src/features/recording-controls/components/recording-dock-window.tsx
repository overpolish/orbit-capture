// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useRef, useEffect, useState } from "react";

import { getGeneralSettings } from "../../settings/api";
import { GeneralSettings } from "../../settings/types";
import {
  cancelRecording,
  finishRecordingDockDrag,
  pauseRecording,
  resumeRecording,
  resizeRecordingDock,
  stopRecording,
} from "../api";
import { selectSnapshot, useRecordingStore } from "../store";
import { useElapsedTime } from "../use-elapsed-time";
import { useRecordingMonitor } from "../use-recording-monitor";

import { RecordingDock } from "./recording-dock";

const report = (action: string) => (error: unknown) => {
  console.error(`Could not ${action} the recording`, error);
};

export function RecordingDockWindow() {
  const [showConfidenceChecks, setShowConfidenceChecks] = useState(true);
  const snapshot = useRecordingStore(selectSnapshot);
  const elapsedMs = useElapsedTime(snapshot);
  const monitor = useRecordingMonitor(showConfidenceChecks);
  const lastWidthRef = useRef(0);
  const resizeToContent = useCallback((width: number) => {
    if (width === lastWidthRef.current) return;
    lastWidthRef.current = width;
    resizeRecordingDock(width).catch(report("resize"));
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void getGeneralSettings().then((settings) => {
      setShowConfidenceChecks(settings.showRecordingConfidenceChecks);
    });
    void listen<GeneralSettings>("settings://changed", ({ payload }) => {
      setShowConfidenceChecks(payload.showRecordingConfidenceChecks);
    }).then((listener) => {
      unlisten = listener;
    });
    return () => unlisten?.();
  }, []);

  return (
    <RecordingDock
      countdownSeconds={snapshot.countdownSecondsRemaining}
      elapsedMs={elapsedMs}
      monitor={showConfidenceChecks ? monitor : undefined}
      onDiscard={() => {
        cancelRecording().catch(report("discard"));
      }}
      onPauseChange={(isPaused) => {
        const action = isPaused ? pauseRecording : resumeRecording;
        action().catch(report(isPaused ? "pause" : "resume"));
      }}
      onPointerUp={() => {
        void finishRecordingDockDrag();
      }}
      onStop={() => {
        stopRecording().catch(report("stop"));
      }}
      onWidthChange={resizeToContent}
      status={snapshot.status}
    />
  );
}
