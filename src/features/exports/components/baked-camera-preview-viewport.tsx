// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useRef } from "react";

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
import {
  OverlayEdge,
  useCameraOverlayInteraction,
} from "./use-camera-overlay-interaction";

const HANDLE_DIRECTIONS: { edges: OverlayEdge[]; x: number; y: number }[] = [
  { edges: ["top", "left"], x: 0, y: 0 },
  { edges: ["top"], x: 0.5, y: 0 },
  { edges: ["top", "right"], x: 1, y: 0 },
  { edges: ["right"], x: 1, y: 0.5 },
  { edges: ["bottom", "right"], x: 1, y: 1 },
  { edges: ["bottom"], x: 0.5, y: 1 },
  { edges: ["bottom", "left"], x: 0, y: 1 },
  { edges: ["left"], x: 0, y: 0.5 },
];

const cursorForEdges = (edges: OverlayEdge[]) => {
  const key = edges.join("-");
  if (key === "top" || key === "bottom") return "ns-resize";
  if (key === "left" || key === "right") return "ew-resize";
  if (key === "top-left" || key === "bottom-right") return "nwse-resize";
  return "nesw-resize";
};

export function BakedCameraPreviewViewport({
  cameraCanvasRef,
  cameraPane,
  isBusy,
  onInteractionEnd,
  onInteractionStart,
  onNeedFullResolution,
  onSettingsChange,
  screenCanvasRef,
  screenPane,
  settings,
}: {
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cameraPane: RecordingPreviewPane;
  isBusy: boolean;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  screenPane: RecordingPreviewPane;
  settings: CameraOverlaySettings;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onNeedFullResolution?: () => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
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
  const controlSize = "calc(8px * var(--preview-inverse-scale, 1))";
  const lineWidth = "calc(2px * var(--preview-inverse-scale, 1))";
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const ringDiameter = `${(ringExtent * 2).toString()}px`;
  const radiusHandleOffset = `calc(${(geometry.radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;

  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({
        height: screenPane.height,
        width: screenPane.width,
      })}
      onNeedFullResolution={onNeedFullResolution}
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
            className="absolute cursor-move touch-none overflow-hidden"
            onPointerDown={(event) => {
              const point = naturalPoint(event);
              if (!point) return;
              begin(event, {
                kind: "frame",
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
          <div
            className="pointer-events-none absolute touch-none"
            style={{
              height: `${geometry.frame.height.toString()}px`,
              left: `${geometry.frame.x.toString()}px`,
              top: `${geometry.frame.y.toString()}px`,
              width: `${geometry.frame.width.toString()}px`,
            }}
          >
            <svg
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 size-full overflow-visible"
            >
              <rect
                fill="none"
                height="100%"
                rx={geometry.radius}
                stroke="white"
                strokeDasharray={`calc(5px * ${inverseScale})`}
                strokeWidth={lineWidth}
                width="100%"
              />
            </svg>
            {HANDLE_DIRECTIONS.map(({ edges, x, y }) => (
              <button
                aria-label={`Resize camera crop ${edges.join(" ")}`}
                className="pointer-events-auto absolute rounded-full border-0 bg-white p-0"
                key={edges.join("-")}
                onPointerDown={(event) => {
                  const point = naturalPoint(event);
                  if (!point) return;
                  begin(event, {
                    edges,
                    kind: "resize",
                    pointerX: point.x,
                    pointerY: point.y,
                  });
                }}
                style={{
                  cursor: cursorForEdges(edges),
                  height: controlSize,
                  left: `${(x * 100).toString()}%`,
                  top: `${(y * 100).toString()}%`,
                  transform: "translate(-50%, -50%)",
                  width: controlSize,
                }}
                type="button"
                {...interaction}
              />
            ))}
            <button
              aria-label="Move camera and crop together"
              className="pointer-events-auto absolute rounded-full border-0 bg-white p-0"
              onPointerDown={(event) => {
                const point = naturalPoint(event);
                if (!point) return;
                begin(event, {
                  kind: "whole",
                  pointerX: point.x - geometry.frame.x,
                  pointerY: point.y - geometry.frame.y,
                });
              }}
              style={{
                height: controlSize,
                left: "50%",
                top: "50%",
                transform: "translate(-50%, -50%)",
                width: controlSize,
              }}
              type="button"
              {...interaction}
            />
            <svg
              className="pointer-events-none absolute overflow-visible"
              style={{
                height: ringDiameter,
                left: "50%",
                top: "50%",
                transform: "translate(-50%, -50%)",
                width: ringDiameter,
              }}
              viewBox={`0 0 ${(ringExtent * 2).toString()} ${(ringExtent * 2).toString()}`}
            >
              <circle
                aria-hidden="true"
                className="fill-none stroke-white"
                cx={ringExtent}
                cy={ringExtent}
                r={Math.max(1, ringExtent - 1)}
                style={{ strokeWidth: lineWidth }}
              />
              <circle
                aria-label={`Scale camera ${Math.round(effectiveScale).toString()} percent`}
                className="pointer-events-auto fill-none stroke-transparent"
                cx={ringExtent}
                cy={ringExtent}
                onPointerDown={(event) => {
                  begin(event, { kind: "scale" });
                }}
                r={Math.max(1, ringExtent - 1)}
                role="button"
                style={{
                  cursor: "nesw-resize",
                  pointerEvents: "stroke",
                  strokeWidth: `calc(10px * ${inverseScale})`,
                }}
                tabIndex={0}
                {...interaction}
              />
            </svg>
            <button
              aria-label={`Camera corner radius ${Math.round(settings.radiusPercent).toString()} percent`}
              className="pointer-events-auto absolute rounded-full border-0 bg-white p-0"
              onPointerDown={(event) => {
                begin(event, { kind: "radius" });
              }}
              style={{
                cursor: "nwse-resize",
                height: controlSize,
                left: radiusHandleOffset,
                top: radiusHandleOffset,
                transform: "translate(-50%, -50%)",
                width: controlSize,
              }}
              type="button"
              {...interaction}
            />
          </div>
          {isBusy ? (
            <div className="pointer-events-none absolute inset-0 bg-content/20 backdrop-blur-sm" />
          ) : null}
        </div>
      )}
      resetKey={`baked:${screenPane.width.toString()}x${screenPane.height.toString()}`}
    />
  );
}
