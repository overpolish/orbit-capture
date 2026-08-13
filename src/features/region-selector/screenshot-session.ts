// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useRecordingInputStore } from "../recording-inputs/store";
import {
  hideRegionSelector,
  listMonitors,
  setRegionSelectorOpacity,
  setRegionSelectorPassthrough,
  setScreenshotRegionSession,
  showRegionSelector,
} from "../recording-sources/api";
import { useRecordingSourceStore } from "../recording-sources/store";
import { Region } from "../recording-sources/types";
import { captureStill } from "../screenshots/api";

/**
 * The screenshot shortcut's borrowing of the region overlay, from opening it
 * in region-edit mode to handing the still to the export window.
 *
 * The session touches nothing the user chose: the recording mode, source and
 * region are all left exactly as they were found.
 */
export const beginScreenshotCapture = async () => {
  const { selectedMonitor, setScreenshotCapture, setSelectedMonitor } =
    useRecordingSourceStore.getState();

  if (!selectedMonitor) {
    const monitors = await listMonitors();
    const monitor = monitors.find((candidate) => candidate.isPrimary);
    if (!monitor) return;
    setSelectedMonitor(monitor);
  }
  // Rust has to know the overlay is allowed on screen before it is asked for:
  // the recording controls may well be hidden behind it.
  await setScreenshotRegionSession(true);
  setScreenshotCapture(true);
};

export const endScreenshotCapture = async () => {
  const { recordingMode, selectedMonitor, setScreenshotCapture } =
    useRecordingSourceStore.getState();

  // Undoing exactly what starting the session did, so the overlay goes back to
  // being the recording region's - or to being off screen.
  await setRegionSelectorPassthrough(true);
  await setScreenshotRegionSession(false);
  await hideRegionSelector();
  await setRegionSelectorOpacity(1);
  setScreenshotCapture(false);
  // With the session flag already cleared, this asks for the overlay on the
  // recording UI's terms: it comes back for a cancelled session, and stays away
  // once an export has taken the screen.
  if (recordingMode === "region" && selectedMonitor) {
    await showRegionSelector(selectedMonitor);
  }
};

/** Captures the region and hands it to the export window, ending the session. */
export const captureScreenshotRegion = (monitorId: number, region: Region) => {
  // The overlay is on top of what is being captured, so it goes invisible for
  // the shot exactly as it does for the magnifier's monitor image.
  setRegionSelectorOpacity(0)
    .then(() =>
      captureStill({
        destination: "export",
        showCursor: useRecordingInputStore.getState().inputs.showCursor,
        target: { kind: "region", monitorId, region },
      }),
    )
    .catch((error: unknown) => {
      console.error("Could not take the screenshot", error);
    })
    .finally(() => {
      void endScreenshotCapture();
    });
};
