// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

import { MonitorDetails, RecordingMode, Region, WindowDetails } from "./types";

const STORE_NAME = "orbit-capture-recording-source";

type RecordingSourceStore = {
  isRegionEditing: boolean;
  recordingMode: RecordingMode;
  region: Region;
  selectedMonitor: MonitorDetails | null;
  selectedWindow: WindowDetails | null;
  setRecordingMode: (mode: RecordingMode) => void;
  setRegion: (region: Region) => void;
  setRegionEditing: (editing: boolean) => void;
  setSelectedMonitor: (monitor: MonitorDetails) => void;
  setSelectedWindow: (window: WindowDetails | null) => void;
};

export const useRecordingSourceStore = create<RecordingSourceStore>()(
  persist(
    (set) => ({
      isRegionEditing: false,
      recordingMode: "screen",
      region: {
        position: { x: 160, y: 90 },
        size: { height: 720, width: 1280 },
      },
      selectedMonitor: null,
      selectedWindow: null,
      setRecordingMode: (recordingMode) => {
        set((state) => ({
          isRegionEditing:
            recordingMode === "region" ? state.isRegionEditing : false,
          recordingMode,
        }));
      },
      setRegion: (region) => {
        set({ region });
      },
      setRegionEditing: (isRegionEditing) => {
        set({ isRegionEditing });
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
