// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef } from "react";

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
  const snapshot = useRecordingStore(selectSnapshot);
  const elapsedMs = useElapsedTime(snapshot);
  const monitor = useRecordingMonitor();
  const lastWidthRef = useRef(0);
  const resizeToContent = useCallback((width: number) => {
    if (width === lastWidthRef.current) return;
    lastWidthRef.current = width;
    resizeRecordingDock(width).catch(report("resize"));
  }, []);

  return (
    <RecordingDock
      elapsedMs={elapsedMs}
      monitor={monitor}
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
