import { create } from "zustand";

import { ExportSnapshot, initialExportSnapshot } from "./types";

type ExportStore = {
  hydrated: boolean;
  setSnapshot: (snapshot: ExportSnapshot) => void;
  snapshot: ExportSnapshot;
};

/** Owned by Rust and broadcast, like the recording snapshot. Not persisted. */
export const useExportStore = create<ExportStore>()((set) => ({
  hydrated: false,
  setSnapshot: (snapshot) => {
    set({ hydrated: true, snapshot });
  },
  snapshot: initialExportSnapshot,
}));

export const selectArtifact = (state: ExportStore) => state.snapshot.artifact;

export const selectDirectory = (state: ExportStore) => state.snapshot.directory;
