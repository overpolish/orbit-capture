import { create } from "zustand";

import { initialRecordingSnapshot, RecordingSnapshot } from "./types";

type RecordingStore = {
  hydrated: boolean;
  setSnapshot: (snapshot: RecordingSnapshot) => void;
  snapshot: RecordingSnapshot;
};

/**
 * Deliberately not persisted. Recording state is owned by Rust and broadcast to
 * every window; mirroring it through localStorage would race across windows.
 */
export const useRecordingStore = create<RecordingStore>()((set) => ({
  hydrated: false,
  setSnapshot: (snapshot) => {
    set({ hydrated: true, snapshot });
  },
  snapshot: initialRecordingSnapshot,
}));

export const selectSnapshot = (state: RecordingStore) => state.snapshot;

export const selectStatus = (state: RecordingStore) => state.snapshot.status;

export const selectIsIdle = (state: RecordingStore) =>
  state.snapshot.status === "idle";

export const selectIsBusy = (state: RecordingStore) =>
  state.snapshot.status === "starting" || state.snapshot.status === "stopping";
