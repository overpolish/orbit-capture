// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  cancelRecording,
  finishRecordingDockDrag,
  pauseRecording,
  resumeRecording,
  stopRecording,
} from "../api";
import { selectSnapshot, useRecordingStore } from "../store";
import { useElapsedTime } from "../use-elapsed-time";

import { RecordingDock } from "./recording-dock";

const report = (action: string) => (error: unknown) => {
  console.error(`Could not ${action} the recording`, error);
};

export function RecordingDockWindow() {
  const snapshot = useRecordingStore(selectSnapshot);
  const elapsedMs = useElapsedTime(snapshot);

  return (
    <RecordingDock
      elapsedMs={elapsedMs}
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
      status={snapshot.status}
    />
  );
}
