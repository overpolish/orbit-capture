// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { create } from "zustand";

import {
  ExportKind,
  ExportSnapshot,
  ExportSnapshots,
  initialExportSnapshot,
} from "./types";

type ExportStore = {
  hydrated: boolean;
  setSnapshot: (snapshot: ExportSnapshot) => void;
  setSnapshots: (snapshots: ExportSnapshots) => void;
  snapshots: ExportSnapshots;
};

/**
 * Owned by Rust and broadcast, like the recording snapshot. Not persisted.
 *
 * Every workspace is held here rather than only this window's: the recording
 * bar reads both, and an export window would otherwise have to filter the
 * broadcast before it could trust what it stored.
 */
export const useExportStore = create<ExportStore>()((set) => ({
  hydrated: false,
  setSnapshot: (snapshot) => {
    set((state) => ({
      hydrated: true,
      snapshots: { ...state.snapshots, [snapshot.workspace]: snapshot },
    }));
  },
  setSnapshots: (snapshots) => {
    set({ hydrated: true, snapshots });
  },
  snapshots: {
    recording: initialExportSnapshot("recording"),
    screenshot: initialExportSnapshot("screenshot"),
  },
}));

export const selectSnapshot = (kind: ExportKind) => (state: ExportStore) =>
  state.snapshots[kind];

export const selectArtifact = (kind: ExportKind) => (state: ExportStore) =>
  state.snapshots[kind].artifact;

export const selectDirectory = (kind: ExportKind) => (state: ExportStore) =>
  state.snapshots[kind].directory;

/**
 * What the recording bar needs. Two booleans rather than one object: a
 * selector that built an object would hand back a fresh identity on every
 * store read and re-render the bar continuously.
 */
export const selectHasPendingRecording = (state: ExportStore) =>
  state.snapshots.recording.artifact !== null;

export const selectHasPendingScreenshot = (state: ExportStore) =>
  state.snapshots.screenshot.artifact !== null;
