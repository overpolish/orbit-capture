// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Crop, MousePointer2, ScanSquare } from "lucide-react";
import { MouseEvent as ReactMouseEvent } from "react";
import { TooltipTrigger } from "react-aria-components";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Keyboard } from "../../../components/base/keyboard/keyboard";
import { Tooltip } from "../../../components/base/tooltip/tooltip";
import {
  cameraOverlayForDimensions,
  defaultCameraOverlay,
} from "../recording-export-settings";
import {
  RecordingOutputSettings,
  resetScreenshotCrop,
  resetScreenshotLayout,
  resetScreenshotTransform,
} from "../screenshot-output";
import {
  CameraOverlaySettings,
  RecordingPreviewPane,
  RecordingVideoTrackId,
} from "../types";

export type RecordingCanvasTool = "canvas" | "crop" | "select" | null;

export function RecordingCanvasTools({
  activeTrack,
  bakeCamera,
  cameraPane,
  isEnabled,
  isFrameEnabled = isEnabled,
  isSelectEnabled = isEnabled,
  onCameraOverlayReset,
  onChange,
  onToolChange,
  outputs,
  screenPane,
  tool,
}: {
  activeTrack: RecordingVideoTrackId | null;
  bakeCamera: boolean;
  isEnabled: boolean;
  onToolChange: (tool: RecordingCanvasTool) => void;
  tool: RecordingCanvasTool;
  cameraPane?: RecordingPreviewPane;
  isFrameEnabled?: boolean;
  isSelectEnabled?: boolean;
  onCameraOverlayReset?: (settings: CameraOverlaySettings) => void;
  onChange?: (
    track: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId],
  ) => void;
  outputs?: RecordingOutputSettings;
  screenPane?: RecordingPreviewPane;
}) {
  const reset = (event: ReactMouseEvent<HTMLSpanElement>) => {
    event.preventDefault();
    if (activeTrack === "camera" && bakeCamera && tool !== "canvas") {
      // The reset must be computed from the real output and camera geometry:
      // generic 16:9 defaults land the crop frame outside the camera image
      // for other aspect ratios, and the compositor's clamped rendering then
      // no longer matches the on-screen controls.
      onCameraOverlayReset?.(
        outputs && cameraPane
          ? cameraOverlayForDimensions({
              cameraHeight: cameraPane.sourceHeight,
              cameraWidth: cameraPane.sourceWidth,
              screenHeight: outputs.primary.height,
              screenWidth: outputs.primary.width,
            })
          : defaultCameraOverlay(),
      );
      return;
    }
    if (!activeTrack || !outputs) return;
    const targetTrack =
      bakeCamera && tool === "canvas" ? "primary" : activeTrack;
    const pane = targetTrack === "primary" ? screenPane : cameraPane;
    if (!pane) return;
    const source = { height: pane.sourceHeight, width: pane.sourceWidth };
    const current = outputs[targetTrack];
    const next =
      tool === "canvas"
        ? resetScreenshotLayout(
            { ...current, height: source.height, width: source.width },
            source,
          )
        : tool === "select"
          ? resetScreenshotTransform(current, source)
          : resetScreenshotCrop(current, source);
    onChange?.(targetTrack, next);
  };
  return (
    <div className="flex items-center gap-1">
      <TooltipTrigger delay={400}>
        <span className="inline-flex" onContextMenu={reset}>
          <ToggleButton
            animation="scale-selected"
            aria-keyshortcuts="V"
            aria-label="Select recording clip"
            isDisabled={!isSelectEnabled}
            isSelected={tool === "select" && isSelectEnabled}
            onChange={(selected) => {
              onToolChange(selected ? "select" : null);
            }}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <MousePointer2 size={15} />
          </ToggleButton>
        </span>
        <Tooltip placement="bottom">
          <span className="flex items-center gap-2">
            Select
            <Keyboard size="xs" variant="tooltip">
              V
            </Keyboard>
          </span>
        </Tooltip>
      </TooltipTrigger>
      <TooltipTrigger delay={400}>
        <span className="inline-flex" onContextMenu={reset}>
          <ToggleButton
            animation="scale-selected"
            aria-keyshortcuts="F"
            aria-label="Resize recording frame"
            isDisabled={!isFrameEnabled}
            isSelected={tool === "canvas" && isFrameEnabled}
            onChange={(selected) => {
              onToolChange(selected ? "canvas" : null);
            }}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <ScanSquare size={15} />
          </ToggleButton>
        </span>
        <Tooltip placement="bottom">
          <span className="flex items-center gap-2">
            Resize frame
            <Keyboard size="xs" variant="tooltip">
              F
            </Keyboard>
          </span>
        </Tooltip>
      </TooltipTrigger>
      <TooltipTrigger delay={400}>
        <span className="inline-flex" onContextMenu={reset}>
          <ToggleButton
            animation="scale-selected"
            aria-keyshortcuts="C"
            aria-label="Crop recording clip"
            isDisabled={!isEnabled}
            isSelected={tool === "crop" && isEnabled}
            onChange={(selected) => {
              onToolChange(selected ? "crop" : null);
            }}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <Crop size={15} />
          </ToggleButton>
        </span>
        <Tooltip placement="bottom">
          <span className="flex items-center gap-2">
            Crop
            <Keyboard size="xs" variant="tooltip">
              C
            </Keyboard>
          </span>
        </Tooltip>
      </TooltipTrigger>
    </div>
  );
}
