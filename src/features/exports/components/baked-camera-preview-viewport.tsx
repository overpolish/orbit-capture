// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject } from "react";

import {
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";

import { NativeRecordingWorkspaceViewport } from "./native-recording-workspace-viewport";
import { RecordingCanvasTool } from "./recording-crop-toggle";

/**
 * The baked composition is a single native pane: the camera overlay is drawn
 * into the primary output by the compositor, so the workspace only needs one
 * marker canvas whose bounds the native surface mirrors.
 */
export function BakedCameraPreviewViewport({
  isBusy,
  outputSettings,
  screenCanvasRef,
  tool,
}: {
  isBusy: boolean;
  outputSettings: ScreenshotOutputSettings;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  tool: RecordingCanvasTool;
}) {
  const output = screenshotOutputDimensions(outputSettings);
  return (
    <NativeRecordingWorkspaceViewport
      ariaLabel="Native baked recording workspace preview"
      isBusy={isBusy}
      isSelecting={tool === "select"}
      panes={[
        {
          height: output.height,
          index: 0,
          label: "Composed recording preview",
          ref: screenCanvasRef,
          width: output.width,
          x: 0,
          y: 0,
        },
      ]}
      workspaceHeight={output.height}
      workspaceWidth={output.width}
    />
  );
}
