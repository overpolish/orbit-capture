// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Crop } from "lucide-react";
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
  resetScreenshotLayout,
} from "../screenshot-output";
import {
  CameraOverlaySettings,
  RecordingPreviewPane,
  RecordingVideoTrackId,
} from "../types";

export function RecordingCropToggle({
  activeTrack,
  bakeCamera,
  cameraPane,
  isEditing,
  isEnabled,
  onCameraOverlayReset,
  onChange,
  onEditingChange,
  outputs,
  screenPane,
}: {
  activeTrack: RecordingVideoTrackId | null;
  bakeCamera: boolean;
  isEditing: boolean;
  isEnabled: boolean;
  onEditingChange: (editing: boolean) => void;
  cameraPane?: RecordingPreviewPane;
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
    if (activeTrack === "camera" && bakeCamera) {
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
    const pane = activeTrack === "primary" ? screenPane : cameraPane;
    if (!pane) return;
    onChange?.(
      activeTrack,
      resetScreenshotLayout(outputs[activeTrack], {
        height: pane.sourceHeight,
        width: pane.sourceWidth,
      }),
    );
  };
  return (
    <TooltipTrigger delay={400}>
      <span className="inline-flex" onContextMenu={reset}>
        <ToggleButton
          aria-keyshortcuts="C"
          aria-label="Edit recording placement and crop"
          isDisabled={!isEnabled}
          isSelected={isEditing && isEnabled}
          onChange={onEditingChange}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <Crop size={15} />
        </ToggleButton>
      </span>
      <Tooltip placement="bottom">
        <span className="flex items-center gap-2">
          Edit placement and crop
          <Keyboard size="xs" variant="tooltip">
            C
          </Keyboard>
        </span>
      </Tooltip>
    </TooltipTrigger>
  );
}
