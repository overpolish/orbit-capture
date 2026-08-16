// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEvent as ReactPointerEvent, RefObject, useRef } from "react";

import {
  cameraOverlayGeometry,
  clamp,
  minimumCameraFrameWidth,
  OverlayRect,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
} from "../camera-overlay-geometry";
import {
  ScreenshotSnapGuide,
  snapScreenshotFrame,
} from "../screenshot-snapping";
import { CameraOverlaySettings, RecordingPreviewPane } from "../types";

type OverlayEdge = "bottom" | "left" | "right" | "top";
export type OverlayAction =
  | { kind: "frame" | "whole"; pointerX: number; pointerY: number }
  | {
      edges: OverlayEdge[];
      kind: "resize" | "transformResize";
      pointerX: number;
      pointerY: number;
    }
  | { kind: "radius" };

const frameInsideCamera = (frame: OverlayRect, camera: OverlayRect) =>
  frame.x >= camera.x &&
  frame.y >= camera.y &&
  frame.x + frame.width <= camera.x + camera.width &&
  frame.y + frame.height <= camera.y + camera.height;

type MoveContext = {
  centered: boolean;
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
  onSnapGuidesChange,
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
  onSnapGuidesChange?: (guides: {
    x?: ScreenshotSnapGuide;
    y?: ScreenshotSnapGuide;
  }) => void;
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
    const frameOnly = action.kind === "frame";
    const minimumX = frameOnly ? start.camera.x : -start.frame.width;
    const maximumX = frameOnly
      ? start.camera.x + start.camera.width - start.frame.width
      : screenPane.width;
    const minimumY = frameOnly ? start.camera.y : -start.frame.height;
    const maximumY = frameOnly
      ? start.camera.y + start.camera.height - start.frame.height
      : screenPane.height;
    let x = clamp(point.x - action.pointerX, minimumX, maximumX);
    let y = clamp(point.y - action.pointerY, minimumY, maximumY);
    if (!frameOnly && snapPosition) {
      const bounds = mediaRef.current?.getBoundingClientRect();
      const snapped = snapScreenshotFrame({
        canvas: screenPane,
        frame: start.frame,
        objects: [],
        position: { x, y },
        thresholdX: (screenPane.width / Math.max(1, bounds?.width ?? 1)) * 8,
        thresholdY: (screenPane.height / Math.max(1, bounds?.height ?? 1)) * 8,
      });
      x = clamp(snapped.position.x, minimumX, maximumX);
      y = clamp(snapped.position.y, minimumY, maximumY);
      onSnapGuidesChange?.(snapped.guides);
    } else {
      onSnapGuidesChange?.({});
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
    action: Extract<OverlayAction, { edges: OverlayEdge[] }>,
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
      left = clamp(point.x, start.camera.x, right - minimumSize);
    if (action.edges.includes("right"))
      right = clamp(
        point.x,
        left + minimumSize,
        start.camera.x + start.camera.width,
      );
    if (action.edges.includes("top"))
      top = clamp(point.y, start.camera.y, bottom - minimumSize);
    if (action.edges.includes("bottom"))
      bottom = clamp(
        point.y,
        top + minimumSize,
        start.camera.y + start.camera.height,
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

  const resizeWhole = (
    action: Extract<OverlayAction, { edges: OverlayEdge[] }>,
    { centered, next, point, start }: MoveContext,
  ) => {
    const { edges } = action;
    const anchorX = centered
      ? start.frame.x + start.frame.width / 2
      : edges.includes("left")
        ? start.frame.x + start.frame.width
        : edges.includes("right")
          ? start.frame.x
          : start.frame.x + start.frame.width / 2;
    const anchorY = centered
      ? start.frame.y + start.frame.height / 2
      : edges.includes("top")
        ? start.frame.y + start.frame.height
        : edges.includes("bottom")
          ? start.frame.y
          : start.frame.y + start.frame.height / 2;
    const handleX = edges.includes("left")
      ? start.frame.x
      : edges.includes("right")
        ? start.frame.x + start.frame.width
        : start.frame.x + start.frame.width / 2;
    const handleY = edges.includes("top")
      ? start.frame.y
      : edges.includes("bottom")
        ? start.frame.y + start.frame.height
        : start.frame.y + start.frame.height / 2;
    const vectorX = handleX - anchorX;
    const vectorY = handleY - anchorY;
    const denominator = vectorX * vectorX + vectorY * vectorY;
    const minimumScale =
      minimumCameraFrameWidth(screenPane, start.frame) / start.frame.width;
    const requestedScale =
      denominator > 0
        ? ((point.x - anchorX) * vectorX + (point.y - anchorY) * vectorY) /
          denominator
        : 1;
    const scale = clamp(requestedScale, minimumScale, 8);
    const transform = (value: number, anchor: number) =>
      anchor + (value - anchor) * scale;
    const cameraCenterX = start.camera.x + start.camera.width / 2;
    const cameraCenterY = start.camera.y + start.camera.height / 2;
    next.frameXPercent =
      (transform(start.frame.x, anchorX) * 100) / screenPane.width;
    next.frameYPercent =
      (transform(start.frame.y, anchorY) * 100) / screenPane.height;
    next.frameWidthPercent =
      (start.frame.width * scale * 100) / screenPane.width;
    next.frameHeightPercent =
      (start.frame.height * scale * 100) / screenPane.height;
    next.cameraWidthPercent =
      (start.camera.width * scale * 100) / screenPane.width;
    next.cameraXPercent =
      (transform(cameraCenterX, anchorX) * 100) / screenPane.width;
    next.cameraYPercent =
      (transform(cameraCenterY, anchorY) * 100) / screenPane.height;
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
      centered: event.altKey,
      next,
      point,
      snapPosition: event.metaKey || event.ctrlKey,
      start,
    };
    if (active.action.kind === "whole" || active.action.kind === "frame") {
      moveFrame(active.action, context);
    } else if (active.action.kind === "resize") {
      resizeFrame(active.action, context);
    } else if (active.action.kind === "transformResize") {
      resizeWhole(active.action, context);
    } else {
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
    }
    onSettingsChange?.(next);
  };

  const finish = (event: ReactPointerEvent) => {
    event.stopPropagation();
    actionRef.current = undefined;
    onSnapGuidesChange?.({});
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
