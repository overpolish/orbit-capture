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
import { CameraOverlaySettings, RecordingPreviewPane } from "../types";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { useCameraOverlayInteraction } from "./use-camera-overlay-interaction";

export function BakedCameraPreviewViewport({
  cameraCanvasRef,
  cameraPane,
  controlsVisible = true,
  isBusy,
  onInteractionEnd,
  onInteractionStart,
  onNeedFullResolution,
  onSettingsChange,
  onZoomChange,
  screenCanvasRef,
  screenPane,
  settings,
  zoomPercent,
}: {
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cameraPane: RecordingPreviewPane;
  isBusy: boolean;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  screenPane: RecordingPreviewPane;
  settings: CameraOverlaySettings;
  controlsVisible?: boolean;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onNeedFullResolution?: () => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
  onZoomChange?: (zoomPercent: number) => void;
  zoomPercent?: number;
}) {
  const mediaRef = useRef<HTMLDivElement | null>(null);
  const geometry = cameraOverlayGeometry(screenPane, cameraPane, settings);
  const effectiveScale =
    (settings.frameWidthPercent * 100) / CAMERA_FRAME_BASE_WIDTH_PERCENT;
  const ringExtent = scaleRingExtent(effectiveScale, SCALE_GIZMO_DIMENSION);

  const { begin, interaction, naturalPoint } = useCameraOverlayInteraction({
    cameraPane,
    mediaRef,
    onInteractionEnd,
    onInteractionStart,
    onSettingsChange,
    screenPane,
    settings,
  });
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const radiusHandleOffset = `calc(${(geometry.radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;

  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({
        height: screenPane.height,
        width: screenPane.width,
      })}
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({ ref, style }) => (
        <div
          className="relative shrink-0 select-none"
          ref={(element) => {
            mediaRef.current = element;
            ref(element);
          }}
          style={{
            ...style,
            height: `${screenPane.height.toString()}px`,
            width: `${screenPane.width.toString()}px`,
          }}
        >
          <canvas
            aria-label="Screen preview"
            className="absolute inset-0 size-full"
            ref={screenCanvasRef}
            role="img"
          />
          <div
            aria-label="Camera crop window"
            className={`absolute touch-none overflow-hidden ${controlsVisible ? "cursor-move" : ""}`}
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
          >
            <canvas
              aria-label="Camera preview"
              className="pointer-events-none absolute max-w-none"
              ref={cameraCanvasRef}
              role="img"
              style={{
                height: `${geometry.camera.height.toString()}px`,
                left: `${(geometry.camera.x - geometry.frame.x).toString()}px`,
                top: `${(geometry.camera.y - geometry.frame.y).toString()}px`,
                width: `${geometry.camera.width.toString()}px`,
              }}
            />
          </div>
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
