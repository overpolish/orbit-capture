import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { MonitorDetails, RecordingMode, WindowDetails } from "./types";

const STORE_NAME = "orbit-capture-recording-source";

type RecordingSourceStore = {
  recordingMode: RecordingMode;
  selectedMonitor: MonitorDetails | null;
  selectedWindow: WindowDetails | null;
  setRecordingMode: (mode: RecordingMode) => void;
  setSelectedMonitor: (monitor: MonitorDetails) => void;
  setSelectedWindow: (window: WindowDetails | null) => void;
};

export const useRecordingSourceStore = create<RecordingSourceStore>()(
  persist(
    (set) => ({
      recordingMode: "screen",
      selectedMonitor: null,
      selectedWindow: null,
      setRecordingMode: (recordingMode) => {
        set({ recordingMode });
      },
      setSelectedMonitor: (selectedMonitor) => {
        set({ selectedMonitor });
      },
      setSelectedWindow: (selectedWindow) => {
        set({ selectedWindow });
      },
    }),
    {
      name: STORE_NAME,
      storage: createJSONStorage(() => localStorage),
    },
  ),
);

export const synchronizeRecordingSourceStore = (event: StorageEvent) => {
  if (event.key === STORE_NAME) {
    void useRecordingSourceStore.persist.rehydrate();
  }
};
