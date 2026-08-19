// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject } from "react";

import {
  RecordingOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";
import { RecordingPreviewPane, RecordingVideoTrackId } from "../types";

import { NativeRecordingWorkspaceViewport } from "./native-recording-workspace-viewport";
import { RecordingCanvasTool } from "./recording-crop-toggle";
import { RECORDING_PREVIEW_PANE_GAP } from "./recording-preview-layout";

type Entry = {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  pane: RecordingPreviewPane;
  trackId: RecordingVideoTrackId;
};

export function RecordingOutputPreviewViewport({
  entries,
  outputs,
  tool,
}: {
  entries: Entry[];
  outputs: RecordingOutputSettings;
  tool: RecordingCanvasTool;
}) {
  const dimensions = entries.map(({ trackId }) =>
    screenshotOutputDimensions(outputs[trackId]),
  );
  const height = dimensions.reduce(
    (maximum, size) => Math.max(maximum, size.height),
    0,
  );
  const width = Math.max(
    1,
    dimensions.reduce((total, size) => total + size.width, 0) +
      Math.max(0, entries.length - 1) * RECORDING_PREVIEW_PANE_GAP,
  );
  let x = 0;
  const panes = entries.map((entry, index) => {
    const size = dimensions[index];
    const pane = {
      height: size.height,
      index: entry.trackId === "primary" ? 0 : 1,
      label: `${entry.pane.kind === "camera" ? "Camera" : "Screen"} composed preview`,
      ref: entry.canvasRef,
      width: size.width,
      x,
      y: (height - size.height) / 2,
    };
    x += size.width + RECORDING_PREVIEW_PANE_GAP;
    return pane;
  });
  return (
    <NativeRecordingWorkspaceViewport
      ariaLabel="Native recording workspace preview"
      isBusy={false}
      isSelecting={tool === "select"}
      panes={panes}
      workspaceHeight={height}
      workspaceWidth={width}
    />
  );
}
