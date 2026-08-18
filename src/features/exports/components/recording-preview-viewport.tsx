// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject } from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";
import { RecordingPreviewLayout } from "../types";
import { usePreviewCapabilities } from "../use-preview-capabilities";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { NativeRecordingWorkspaceViewport } from "./native-recording-workspace-viewport";

export function RecordingPreviewViewport({
  canvasRefs,
  isBusy,
  layout,
  onNeedFullResolution,
  onZoomChange,
  zoomPercent,
}: {
  canvasRefs: RefObject<HTMLCanvasElement | null>[];
  isBusy: boolean;
  layout: RecordingPreviewLayout;
  onNeedFullResolution?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  zoomPercent?: number;
}) {
  const capabilities = usePreviewCapabilities();
  if (
    capabilities?.nativeRecordingPreview === true &&
    capabilities.nativeWorkspaceEditor
  ) {
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
  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({ height: layout.height, width: layout.width })}
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({ ref, style }) => (
        <div
          className="relative shrink-0 select-none"
          ref={ref}
          style={{
            ...style,
            height: `${layout.height.toString()}px`,
            width: `${layout.width.toString()}px`,
          }}
        >
          {layout.panes.map((pane, index) => (
            <canvas
              aria-label={`${pane.kind === "camera" ? "Camera" : "Screen"} preview`}
              className="absolute max-w-none"
              key={pane.kind}
              ref={canvasRefs[index]}
              role="img"
              style={{
                height: `${pane.height.toString()}px`,
                left: `${pane.x.toString()}px`,
                top: `${pane.y.toString()}px`,
                width: `${pane.width.toString()}px`,
              }}
            />
          ))}
          <Overlay
            blur="sm"
            className="pointer-events-none"
            contained
            isOpen={isBusy}
          >
            <CircularProgressBar
              aria-label="Preparing the preview"
              isIndeterminate
              size={32}
              strokeWidth={10}
            />
          </Overlay>
        </div>
      )}
      resetKey={`recording:${layout.width.toString()}x${layout.height.toString()}:${layout.panes.map((pane) => pane.kind).join("-")}`}
      zoomPercent={zoomPercent}
    />
  );
}
