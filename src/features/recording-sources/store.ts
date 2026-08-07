import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { MonitorDetails } from "./types";

const STORE_NAME = "orbit-capture-recording-source";

type RecordingSourceStore = {
  selectedMonitor: MonitorDetails | null;
  setSelectedMonitor: (monitor: MonitorDetails) => void;
};

export const useRecordingSourceStore = create<RecordingSourceStore>()(
  persist(
    (set) => ({
      selectedMonitor: null,
      setSelectedMonitor: (selectedMonitor) => {
        set({ selectedMonitor });
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
