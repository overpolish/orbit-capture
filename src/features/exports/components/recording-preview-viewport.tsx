// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject } from "react";

import { RecordingPreviewLayout } from "../types";

import { NativeRecordingWorkspaceViewport } from "./native-recording-workspace-viewport";

export function RecordingPreviewViewport({
  canvasRefs,
  isBusy,
  layout,
}: {
  canvasRefs: RefObject<HTMLCanvasElement | null>[];
  isBusy: boolean;
  layout: RecordingPreviewLayout;
}) {
  return (
    <NativeRecordingWorkspaceViewport
      ariaLabel="Native recording workspace preview"
      isBusy={isBusy}
      panes={layout.panes.map((pane, index) => ({
        height: pane.height,
        index,
        label: `${pane.kind === "camera" ? "Camera" : "Screen"} preview`,
        ref: canvasRefs[index],
        width: pane.width,
        x: pane.x,
        y: pane.y,
      }))}
      workspaceHeight={layout.height}
      workspaceWidth={layout.width}
    />
  );
}
