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

/** Mirrors the screenshot layer's alt drag: the canvas grows around the move. */
export type CameraOverlayAutoFit = {
  autoFitCanvas: boolean;
  autoFitStarted: boolean;
  output: { height: number; width: number };
  settings: CameraOverlaySettings;
};

type MoveContext = {
  autoFitCanvas: boolean;
  centered: boolean;
  next: CameraOverlaySettings;
  point: { x: number; y: number };
  pointerDelta: { x: number; y: number };
  screen: RecordingPreviewPane;
  snapPosition: boolean;
  start: ReturnType<typeof cameraOverlayGeometry>;
};

type ActiveGesture = {
  action: OverlayAction;
  autoFitCanvas: boolean;
  clientX: number;
  clientY: number;
  scaleX: number;
  scaleY: number;
  screen: RecordingPreviewPane;
  settings: CameraOverlaySettings;
};

export const useCameraOverlayInteraction = ({
  cameraPane,
  mediaRef,
  onAutoFitCanvas,
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
  onAutoFitCanvas?: (
    change: CameraOverlayAutoFit,
  ) => CameraOverlaySettings | undefined;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onSettingsChange?: (settings: CameraOverlaySettings) => void;
  onSnapGuidesChange?: (guides: {
    x?: ScreenshotSnapGuide;
    y?: ScreenshotSnapGuide;
  }) => void;
}) => {
  const actionRef = useRef<ActiveGesture | undefined>(undefined);

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
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return;
    onInteractionStart?.();
    actionRef.current = {
      action,
      autoFitCanvas: event.altKey,
      clientX: event.clientX,
      clientY: event.clientY,
      scaleX: screenPane.width / bounds.width,
      scaleY: screenPane.height / bounds.height,
      screen: screenPane,
      settings,
    };
  };

  const moveFrame = (
    action: Extract<OverlayAction, { kind: "frame" | "whole" }>,
    {
      autoFitCanvas,
      next,
      point,
      pointerDelta,
      screen,
      snapPosition,
      start,
    }: MoveContext,
  ) => {
    const frameOnly = action.kind === "frame";
    const minimumX = frameOnly ? start.camera.x : -start.frame.width;
    const maximumX = frameOnly
      ? start.camera.x + start.camera.width - start.frame.width
      : screen.width;
    const minimumY = frameOnly ? start.camera.y : -start.frame.height;
    const maximumY = frameOnly
      ? start.camera.y + start.camera.height - start.frame.height
      : screen.height;
    // A whole move tracks pointer displacement rather than its position: an
    // auto-fitting canvas moves the origin underneath the gesture.
    const rawX = frameOnly
      ? point.x - action.pointerX
      : start.frame.x + pointerDelta.x;
    const rawY = frameOnly
      ? point.y - action.pointerY
      : start.frame.y + pointerDelta.y;
    // The canvas follows the frame out while alt is held, so leaving it is
    // the point of the gesture rather than something to clamp away.
    const hold = (value: number, minimum: number, maximum: number) =>
      autoFitCanvas && !frameOnly ? value : clamp(value, minimum, maximum);
    let x = hold(rawX, minimumX, maximumX);
    let y = hold(rawY, minimumY, maximumY);
    if (!frameOnly && snapPosition) {
      const bounds = mediaRef.current?.getBoundingClientRect();
      const snapped = snapScreenshotFrame({
        canvas: screen,
        frame: start.frame,
        objects: [],
        position: { x, y },
        thresholdX: (screen.width / Math.max(1, bounds?.width ?? 1)) * 8,
        thresholdY: (screen.height / Math.max(1, bounds?.height ?? 1)) * 8,
      });
      x = hold(snapped.position.x, minimumX, maximumX);
      y = hold(snapped.position.y, minimumY, maximumY);
      onSnapGuidesChange?.(snapped.guides);
    } else {
      onSnapGuidesChange?.({});
    }
    const deltaX = x - start.frame.x;
    const deltaY = y - start.frame.y;
    next.frameXPercent = (x * 100) / screen.width;
    next.frameYPercent = (y * 100) / screen.height;
    if (!frameOnly) {
      next.cameraXPercent += (deltaX * 100) / screen.width;
      next.cameraYPercent += (deltaY * 100) / screen.height;
    }
  };

  const resizeFrame = (
    action: Extract<OverlayAction, { edges: OverlayEdge[] }>,
    { next, point, screen, start }: MoveContext,
  ) => {
    const minimumSize = Math.max(
      3,
      40 * (screen.width / (mediaRef.current?.clientWidth || 1)),
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
    next.frameXPercent = (left * 100) / screen.width;
    next.frameYPercent = (top * 100) / screen.height;
    next.frameWidthPercent = (frame.width * 100) / screen.width;
    next.frameHeightPercent = (frame.height * 100) / screen.height;
  };

  const resizeWhole = (
    action: Extract<OverlayAction, { edges: OverlayEdge[] }>,
    { centered, next, point, screen, start }: MoveContext,
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
      minimumCameraFrameWidth(screen, start.frame) / start.frame.width;
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
      (transform(start.frame.x, anchorX) * 100) / screen.width;
    next.frameYPercent =
      (transform(start.frame.y, anchorY) * 100) / screen.height;
    next.frameWidthPercent = (start.frame.width * scale * 100) / screen.width;
    next.frameHeightPercent =
      (start.frame.height * scale * 100) / screen.height;
    next.cameraWidthPercent = (start.camera.width * scale * 100) / screen.width;
    next.cameraXPercent =
      (transform(cameraCenterX, anchorX) * 100) / screen.width;
    next.cameraYPercent =
      (transform(cameraCenterY, anchorY) * 100) / screen.height;
  };

  const move = (event: ReactPointerEvent) => {
    let active = actionRef.current;
    const point = naturalPoint(event);
    if (!active || !point) return;
    event.preventDefault();
    event.stopPropagation();
    const autoFitCanvas = event.altKey;
    let autoFitStarted = false;
    // Toggling alt mid-gesture changes the canvas the move is measured in, so
    // the gesture restarts from what is on screen right now.
    if (
      active.action.kind === "whole" &&
      active.autoFitCanvas !== autoFitCanvas
    ) {
      const bounds = mediaRef.current?.getBoundingClientRect();
      if (!bounds || bounds.width === 0 || bounds.height === 0) return;
      autoFitStarted = autoFitCanvas;
      active = {
        ...active,
        autoFitCanvas,
        clientX: event.clientX,
        clientY: event.clientY,
        scaleX: screenPane.width / bounds.width,
        scaleY: screenPane.height / bounds.height,
        screen: screenPane,
        settings,
      };
      actionRef.current = active;
    }
    const screen = active.screen;
    const start = cameraOverlayGeometry(screen, cameraPane, active.settings);
    const next = { ...active.settings };
    const context = {
      autoFitCanvas,
      centered: event.altKey,
      next,
      point,
      pointerDelta: {
        x: (event.clientX - active.clientX) * active.scaleX,
        y: (event.clientY - active.clientY) * active.scaleY,
      },
      screen,
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
        (mediaRef.current?.getBoundingClientRect().width || screen.width) /
        screen.width;
      const radius = clamp(
        ((point.x - start.frame.x + point.y - start.frame.y) / 2 -
          RADIUS_HANDLE_INSET / displayScale) /
          RADIUS_HANDLE_TRAVEL,
        0,
        shortest / 2,
      );
      next.radiusPercent = (radius * 100) / shortest;
    }
    // The auto-fit owner grows the canvas around the move and hands back the
    // overlay re-expressed in it; every other action commits as it stands.
    const committed =
      (active.action.kind === "whole"
        ? onAutoFitCanvas?.({
            autoFitCanvas,
            autoFitStarted,
            output: { height: screen.height, width: screen.width },
            settings: next,
          })
        : undefined) ?? next;
    onSettingsChange?.(committed);
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
