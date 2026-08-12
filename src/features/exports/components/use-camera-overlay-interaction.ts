// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEvent as ReactPointerEvent, RefObject, useRef } from "react";

import {
  CAMERA_FRAME_BASE_WIDTH_PERCENT,
  cameraOverlayGeometry,
  clamp,
  minimumCameraFrameWidth,
  OverlayRect,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
  SCALE_GIZMO_DIMENSION,
  scalePercentFromRingExtent,
  snapCameraFramePosition,
} from "../camera-overlay-geometry";
import { CameraOverlaySettings, RecordingPreviewPane } from "../types";

type OverlayEdge = "bottom" | "left" | "right" | "top";
export type OverlayAction =
  | { kind: "frame" | "whole"; pointerX: number; pointerY: number }
  | {
      edges: OverlayEdge[];
      kind: "resize";
      pointerX: number;
      pointerY: number;
    }
  | { kind: "radius" }
  | { kind: "scale" };

const frameInsideCamera = (frame: OverlayRect, camera: OverlayRect) =>
  frame.x >= camera.x &&
  frame.y >= camera.y &&
  frame.x + frame.width <= camera.x + camera.width &&
  frame.y + frame.height <= camera.y + camera.height;

type MoveContext = {
  next: CameraOverlaySettings;
  point: { x: number; y: number };
  snapPosition: boolean;
  start: ReturnType<typeof cameraOverlayGeometry>;
};

export const useCameraOverlayInteraction = ({
  cameraPane,
  mediaRef,
  onInteractionEnd,
  onInteractionStart,
  onSettingsChange,
  screenPane,
  settings,
}: {
  cameraPane: RecordingPreviewPane;
  mediaRef: RefObject<HTMLDivElement | null>;
  screenPane: RecordingPreviewPane;
  settings: CameraOverlaySettings;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
}) => {
  const actionRef = useRef<
    { action: OverlayAction; settings: CameraOverlaySettings } | undefined
  >(undefined);

  const naturalPoint = (event: ReactPointerEvent) => {
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return null;
    return {
      x: ((event.clientX - bounds.left) * screenPane.width) / bounds.width,
      y: ((event.clientY - bounds.top) * screenPane.height) / bounds.height,
    };
  };

  const begin = (event: ReactPointerEvent, action: OverlayAction) => {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    onInteractionStart?.();
    actionRef.current = { action, settings };
  };

  const moveFrame = (
    action: Extract<OverlayAction, { kind: "frame" | "whole" }>,
    { next, point, snapPosition, start }: MoveContext,
  ) => {
    const cameraCenterX = start.camera.x + start.camera.width / 2;
    const cameraCenterY = start.camera.y + start.camera.height / 2;
    const frameOnly = action.kind === "frame";
    const minimumX = frameOnly
      ? Math.max(0, start.camera.x)
      : Math.max(0, start.frame.x - cameraCenterX);
    const maximumX = frameOnly
      ? Math.min(
          screenPane.width - start.frame.width,
          start.camera.x + start.camera.width - start.frame.width,
        )
      : Math.min(
          screenPane.width - start.frame.width,
          start.frame.x + screenPane.width - cameraCenterX,
        );
    const minimumY = frameOnly
      ? Math.max(0, start.camera.y)
      : Math.max(0, start.frame.y - cameraCenterY);
    const maximumY = frameOnly
      ? Math.min(
          screenPane.height - start.frame.height,
          start.camera.y + start.camera.height - start.frame.height,
        )
      : Math.min(
          screenPane.height - start.frame.height,
          start.frame.y + screenPane.height - cameraCenterY,
        );
    let x = clamp(point.x - action.pointerX, minimumX, maximumX);
    let y = clamp(point.y - action.pointerY, minimumY, maximumY);
    if (!frameOnly && snapPosition) {
      const snapped = snapCameraFramePosition({
        frame: start.frame,
        position: { x, y },
        screen: screenPane,
      });
      x = clamp(snapped.x, minimumX, maximumX);
      y = clamp(snapped.y, minimumY, maximumY);
    }
    const deltaX = x - start.frame.x;
    const deltaY = y - start.frame.y;
    next.frameXPercent = (x * 100) / screenPane.width;
    next.frameYPercent = (y * 100) / screenPane.height;
    if (!frameOnly) {
      next.cameraXPercent += (deltaX * 100) / screenPane.width;
      next.cameraYPercent += (deltaY * 100) / screenPane.height;
    }
  };

  const resizeFrame = (
    action: Extract<OverlayAction, { kind: "resize" }>,
    { next, point, start }: MoveContext,
  ) => {
    const minimumSize = Math.max(
      3,
      40 * (screenPane.width / (mediaRef.current?.clientWidth || 1)),
    );
    let left = start.frame.x;
    let top = start.frame.y;
    let right = left + start.frame.width;
    let bottom = top + start.frame.height;
    if (action.edges.includes("left"))
      left = clamp(point.x, Math.max(0, start.camera.x), right - minimumSize);
    if (action.edges.includes("right"))
      right = clamp(
        point.x,
        left + minimumSize,
        Math.min(screenPane.width, start.camera.x + start.camera.width),
      );
    if (action.edges.includes("top"))
      top = clamp(point.y, Math.max(0, start.camera.y), bottom - minimumSize);
    if (action.edges.includes("bottom"))
      bottom = clamp(
        point.y,
        top + minimumSize,
        Math.min(screenPane.height, start.camera.y + start.camera.height),
      );
    const frame = {
      height: bottom - top,
      width: right - left,
      x: left,
      y: top,
    };
    if (!frameInsideCamera(frame, start.camera)) return;
    next.frameXPercent = (left * 100) / screenPane.width;
    next.frameYPercent = (top * 100) / screenPane.height;
    next.frameWidthPercent = (frame.width * 100) / screenPane.width;
    next.frameHeightPercent = (frame.height * 100) / screenPane.height;
  };

  const move = (event: ReactPointerEvent) => {
    const active = actionRef.current;
    const point = naturalPoint(event);
    if (!active || !point) return;
    event.preventDefault();
    event.stopPropagation();
    const start = cameraOverlayGeometry(
      screenPane,
      cameraPane,
      active.settings,
    );
    const next = { ...active.settings };
    const context = {
      next,
      point,
      snapPosition: event.metaKey || event.ctrlKey,
      start,
    };
    if (active.action.kind === "whole" || active.action.kind === "frame") {
      moveFrame(active.action, context);
    } else if (active.action.kind === "resize") {
      resizeFrame(active.action, context);
    } else if (active.action.kind === "radius") {
      const shortest = Math.min(start.frame.width, start.frame.height);
      const displayScale =
        (mediaRef.current?.getBoundingClientRect().width || screenPane.width) /
        screenPane.width;
      const radius = clamp(
        ((point.x - start.frame.x + point.y - start.frame.y) / 2 -
          RADIUS_HANDLE_INSET / displayScale) /
          RADIUS_HANDLE_TRAVEL,
        0,
        shortest / 2,
      );
      next.radiusPercent = (radius * 100) / shortest;
    } else {
      const extent = Math.hypot(
        point.x - (start.frame.x + start.frame.width / 2),
        point.y - (start.frame.y + start.frame.height / 2),
      );
      const scale = scalePercentFromRingExtent(extent, SCALE_GIZMO_DIMENSION);
      const frameCenterX = start.frame.x + start.frame.width / 2;
      const frameCenterY = start.frame.y + start.frame.height / 2;
      const maximumWidth = Math.min(
        frameCenterX * 2,
        (screenPane.width - frameCenterX) * 2,
        (frameCenterY * 2 * start.frame.width) / start.frame.height,
        ((screenPane.height - frameCenterY) * 2 * start.frame.width) /
          start.frame.height,
      );
      const minimumWidth = minimumCameraFrameWidth(screenPane, start.frame);
      const requestedWidth =
        (screenPane.width * CAMERA_FRAME_BASE_WIDTH_PERCENT * scale) / 10_000;
      const frameWidth = clamp(requestedWidth, minimumWidth, maximumWidth);
      const scaleFactor = frameWidth / start.frame.width;
      const frameHeight = start.frame.height * scaleFactor;
      const cameraCenterX = start.camera.x + start.camera.width / 2;
      const cameraCenterY = start.camera.y + start.camera.height / 2;
      next.frameWidthPercent = (frameWidth * 100) / screenPane.width;
      next.frameHeightPercent = (frameHeight * 100) / screenPane.height;
      next.frameXPercent =
        ((frameCenterX - frameWidth / 2) * 100) / screenPane.width;
      next.frameYPercent =
        ((frameCenterY - frameHeight / 2) * 100) / screenPane.height;
      next.cameraWidthPercent =
        (start.camera.width * scaleFactor * 100) / screenPane.width;
      next.cameraXPercent =
        (frameCenterX + (cameraCenterX - frameCenterX) * scaleFactor) *
        (100 / screenPane.width);
      next.cameraYPercent =
        (frameCenterY + (cameraCenterY - frameCenterY) * scaleFactor) *
        (100 / screenPane.height);
    }
    onSettingsChange?.(next);
  };

  const finish = (event: ReactPointerEvent) => {
    event.stopPropagation();
    actionRef.current = undefined;
    onInteractionEnd?.();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (
      event.currentTarget instanceof HTMLElement ||
      event.currentTarget instanceof SVGElement
    )
      event.currentTarget.blur();
  };

  return {
    begin,
    interaction: {
      onPointerCancel: finish,
      onPointerMove: move,
      onPointerUp: finish,
    },
    naturalPoint,
  };
};
