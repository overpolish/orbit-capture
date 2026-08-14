// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useRef } from "react";

import { TransformControls } from "../../../components/shared/canvas-tools/transform-controls";
import {
  CAMERA_FRAME_BASE_WIDTH_PERCENT,
  cameraOverlayGeometry,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
  SCALE_GIZMO_DIMENSION,
  scaleRingExtent,
} from "../camera-overlay-geometry";
import {
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";
import { CameraOverlaySettings, RecordingPreviewPane } from "../types";
import { usePreviewCapabilities } from "../use-preview-capabilities";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { ScreenshotPreviewLayer } from "./screenshot-preview-layer";
import { useCameraOverlayInteraction } from "./use-camera-overlay-interaction";

export function BakedCameraPreviewViewport({
  cameraPane,
  controlsVisible = true,
  isBusy,
  onInteractionEnd,
  onInteractionStart,
  onNeedFullResolution,
  onOutputChange,
  onSettingsChange,
  onZoomChange,
  outputEditing = false,
  outputSettings,
  screenCanvasRef,
  screenPane,
  settings,
  zoomPercent,
}: {
  cameraPane: RecordingPreviewPane;
  isBusy: boolean;
  outputSettings: ScreenshotOutputSettings;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  screenPane: RecordingPreviewPane;
  settings: CameraOverlaySettings;
  controlsVisible?: boolean;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onNeedFullResolution?: () => void;
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
  onZoomChange?: (zoomPercent: number) => void;
  outputEditing?: boolean;
  zoomPercent?: number;
}) {
  const mediaRef = useRef<HTMLDivElement | null>(null);
  const outputRef = useRef<HTMLDivElement | null>(null);
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  const output = screenshotOutputDimensions(outputSettings);
  const outputPane = {
    ...screenPane,
    height: output.height,
    sourceHeight: output.height,
    sourceWidth: output.width,
    width: output.width,
  };
  const geometry = cameraOverlayGeometry(outputPane, cameraPane, settings);
  const effectiveScale =
    (settings.frameWidthPercent * 100) / CAMERA_FRAME_BASE_WIDTH_PERCENT;
  const ringExtent = scaleRingExtent(effectiveScale, SCALE_GIZMO_DIMENSION);

  const { begin, interaction, naturalPoint } = useCameraOverlayInteraction({
    cameraPane,
    mediaRef,
    onInteractionEnd,
    onInteractionStart,
    onSettingsChange,
    screenPane: outputPane,
    settings,
  });
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const radiusHandleOffset = `calc(${(geometry.radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;

  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({
        height: output.height,
        width: output.width,
      })}
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({ ref, style }) => (
        <div
          className="relative shrink-0 select-none"
          ref={(element) => {
            outputRef.current = element;
            ref(element);
          }}
          style={{
            ...style,
            height: `${output.height.toString()}px`,
            width: `${output.width.toString()}px`,
          }}
        >
          <canvas
            aria-label="Native composed recording preview"
            className={`absolute inset-0 size-full max-w-none ${nativeSurface ? "opacity-0" : ""}`}
            ref={screenCanvasRef}
            role="img"
          />
          <ScreenshotPreviewLayer
            isEditing={outputEditing}
            onOutputChange={onOutputChange}
            onRadiusChange={(radiusPercent) =>
              onOutputChange?.({ ...outputSettings, radiusPercent })
            }
            output={output}
            outputRef={outputRef}
            previewCanvasRef={screenCanvasRef}
            radiusPercent={outputSettings.radiusPercent}
            settings={outputSettings}
            source={{
              height: screenPane.sourceHeight,
              width: screenPane.sourceWidth,
            }}
          />
          <div className="pointer-events-none absolute inset-0" ref={mediaRef}>
            <div
              aria-label="Camera crop window"
              className={`pointer-events-auto absolute touch-none overflow-hidden ${controlsVisible ? "cursor-move" : ""}`}
              onPointerDown={(event) => {
                if (!controlsVisible) return;
                const point = naturalPoint(event);
                if (!point) return;
                begin(event, {
                  kind: "whole",
                  pointerX: point.x - geometry.frame.x,
                  pointerY: point.y - geometry.frame.y,
                });
              }}
              role="group"
              style={{
                borderRadius: `${geometry.radius.toString()}px`,
                height: `${geometry.frame.height.toString()}px`,
                left: `${geometry.frame.x.toString()}px`,
                top: `${geometry.frame.y.toString()}px`,
                width: `${geometry.frame.width.toString()}px`,
              }}
              {...interaction}
            ></div>
            {controlsVisible ? (
              <TransformControls
                frame={geometry.frame}
                interaction={interaction}
                inverseScale={inverseScale}
                move={{
                  label: "Move camera crop",
                  onPointerDown: (event) => {
                    const point = naturalPoint(event);
                    if (!point) return;
                    begin(event, {
                      kind: "frame",
                      pointerX: point.x - geometry.frame.x,
                      pointerY: point.y - geometry.frame.y,
                    });
                  },
                }}
                radius={geometry.radius}
                radiusHandle={{
                  cursor: "nwse-resize",
                  label: `Camera corner radius ${Math.round(settings.radiusPercent).toString()} percent`,
                  left: radiusHandleOffset,
                  onPointerDown: (event) => {
                    begin(event, { kind: "radius" });
                  },
                  top: radiusHandleOffset,
                }}
                resize={{
                  label: (edges) => `Resize camera crop ${edges.join(" ")}`,
                  onPointerDown: (edges) => (event) => {
                    const point = naturalPoint(event);
                    if (!point) return;
                    begin(event, {
                      edges,
                      kind: "resize",
                      pointerX: point.x,
                      pointerY: point.y,
                    });
                  },
                }}
                scaleRing={{
                  cursor: "nesw-resize",
                  extent: ringExtent,
                  label: `Scale camera ${Math.round(effectiveScale).toString()} percent`,
                  onPointerDown: (event) => {
                    begin(event, { kind: "scale" });
                  },
                }}
              />
            ) : null}
          </div>
          {isBusy ? (
            <div className="pointer-events-none absolute inset-0 bg-content/20 backdrop-blur-sm" />
          ) : null}
        </div>
      )}
      resetKey={`baked:${screenPane.width.toString()}x${screenPane.height.toString()}`}
      zoomPercent={zoomPercent}
    />
  );
}
