// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  PointerEvent as ReactPointerEvent,
  RefObject,
  useEffect,
  useRef,
} from "react";
import { flushSync } from "react-dom";

import { TransformControls } from "../../../components/shared/canvas-tools/transform-controls";
import {
  resizeScreenshotCanvas,
  ScreenshotWorkspaceOutputSettings,
  screenshotWorkspaceItemOutput,
} from "../screenshot-output";
import { useExportEditGesture } from "../use-export-edit-history";

import type { TransformEdge as Edge } from "../../../components/shared/canvas-tools/transform-handles";

const MINIMUM_CANVAS_SIZE = 64;
const MAXIMUM_CANVAS_PIXELS = 120_000_000;

type ResizeGesture = {
  clientX: number;
  clientY: number;
  edges: Edge[];
  scaleX: number;
  scaleY: number;
  settings: ScreenshotWorkspaceOutputSettings;
};

type ResizePointer = {
  altKey: boolean;
  clientX: number;
  clientY: number;
};

const resizeAxis = ({
  centered,
  delta,
  farEdge,
  nearEdge,
  size,
}: {
  centered: boolean;
  delta: number;
  farEdge: boolean;
  nearEdge: boolean;
  size: number;
}) => {
  let near = 0;
  let far = size;
  if (nearEdge) {
    const movement = Math.min(
      centered ? (size - MINIMUM_CANVAS_SIZE) / 2 : size - MINIMUM_CANVAS_SIZE,
      delta,
    );
    near = movement;
    if (centered) far = size - movement;
  } else if (farEdge) {
    const movement = Math.max(
      centered ? -(size - MINIMUM_CANVAS_SIZE) / 2 : MINIMUM_CANVAS_SIZE - size,
      delta,
    );
    far = size + movement;
    if (centered) near = -movement;
  }
  return { far, near };
};

const resizeAxisToSize = ({
  centered,
  farEdge,
  nearEdge,
  nextSize,
  size,
}: {
  centered: boolean;
  farEdge: boolean;
  nearEdge: boolean;
  nextSize: number;
  size: number;
}) => {
  if (centered && (nearEdge || farEdge)) {
    const inset = (size - nextSize) / 2;
    return { far: size - inset, near: inset };
  }
  if (nearEdge) return { far: size, near: size - nextSize };
  if (farEdge) return { far: nextSize, near: 0 };
  return { far: size, near: 0 };
};

/** Handle-only controls for resizing the exported canvas around its content. */
export function ScreenshotCanvasControl({
  items,
  mediaRef,
  onBoundsChange,
  onChange,
  onResizeEnd,
  onResizeStart,
  output,
  settings,
}: {
  items: { height: number; id: number; width: number }[];
  mediaRef: RefObject<HTMLDivElement | null>;
  output: { height: number; width: number };
  settings: ScreenshotWorkspaceOutputSettings;
  onBoundsChange?: (bounds: {
    height: number;
    originX: number;
    originY: number;
    width: number;
  }) => void;
  onChange?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onResizeEnd?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onResizeStart?: () => void;
}) {
  const editGesture = useExportEditGesture();
  const activeRef = useRef<ResizeGesture | null>(null);
  const draftRef = useRef(settings);
  const frameRef = useRef<number | null>(null);
  const pendingPointerRef = useRef<ResizePointer | null>(null);
  draftRef.current = settings;

  useEffect(
    () => () => {
      if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    },
    [],
  );

  const begin = (edges: Edge[]) => (event: ReactPointerEvent) => {
    if (event.button !== 0) return;
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    editGesture.beginGesture();
    onResizeStart?.();
    activeRef.current = {
      clientX: event.clientX,
      clientY: event.clientY,
      edges,
      scaleX: output.width / bounds.width,
      scaleY: output.height / bounds.height,
      settings: draftRef.current,
    };
  };
  const applyMove = (pointer: ResizePointer) => {
    const active = activeRef.current;
    if (!active) return;
    const startWidth = active.settings.width;
    const startHeight = active.settings.height;
    const deltaX = (pointer.clientX - active.clientX) * active.scaleX;
    const deltaY = (pointer.clientY - active.clientY) * active.scaleY;
    const horizontal = resizeAxis({
      centered: pointer.altKey,
      delta: deltaX,
      farEdge: active.edges.includes("right"),
      nearEdge: active.edges.includes("left"),
      size: startWidth,
    });
    const vertical = resizeAxis({
      centered: pointer.altKey,
      delta: deltaY,
      farEdge: active.edges.includes("bottom"),
      nearEdge: active.edges.includes("top"),
      size: startHeight,
    });
    const horizontalActive = active.edges.some(
      (edge) => edge === "left" || edge === "right",
    );
    const verticalActive = active.edges.some(
      (edge) => edge === "bottom" || edge === "top",
    );
    let nextWidth = Math.max(
      MINIMUM_CANVAS_SIZE,
      horizontal.far - horizontal.near,
    );
    let nextHeight = Math.max(
      MINIMUM_CANVAS_SIZE,
      vertical.far - vertical.near,
    );
    if (nextWidth * nextHeight > MAXIMUM_CANVAS_PIXELS) {
      if (horizontalActive && verticalActive) {
        const scale = Math.sqrt(
          MAXIMUM_CANVAS_PIXELS / (nextWidth * nextHeight),
        );
        nextWidth = Math.floor(nextWidth * scale);
        nextHeight = Math.floor(MAXIMUM_CANVAS_PIXELS / nextWidth);
      } else if (horizontalActive) {
        nextWidth = Math.floor(MAXIMUM_CANVAS_PIXELS / nextHeight);
      } else if (verticalActive) {
        nextHeight = Math.floor(MAXIMUM_CANVAS_PIXELS / nextWidth);
      }
    }
    const constrainedHorizontal = resizeAxisToSize({
      centered: pointer.altKey,
      farEdge: active.edges.includes("right"),
      nearEdge: active.edges.includes("left"),
      nextSize: Math.round(nextWidth),
      size: startWidth,
    });
    const constrainedVertical = resizeAxisToSize({
      centered: pointer.altKey,
      farEdge: active.edges.includes("bottom"),
      nearEdge: active.edges.includes("top"),
      nextSize: Math.round(nextHeight),
      size: startHeight,
    });
    const bounds = {
      height: Math.round(nextHeight),
      originX: constrainedHorizontal.near,
      originY: constrainedVertical.near,
      width: Math.round(nextWidth),
    };
    const next: ScreenshotWorkspaceOutputSettings = {
      ...active.settings,
      height: bounds.height,
      items: active.settings.items.map((itemOutput) => {
        const item = items.find((candidate) => candidate.id === itemOutput.id);
        return item
          ? {
              ...itemOutput,
              output: resizeScreenshotCanvas(
                item,
                screenshotWorkspaceItemOutput(active.settings, itemOutput.id),
                bounds,
              ),
            }
          : itemOutput;
      }),
      width: bounds.width,
    };
    draftRef.current = next;
    // Canvas geometry, OSCs and the native preview must observe the same
    // dimensions in this pointer frame. Letting React batch this update until
    // after the geometry callback briefly stretches the previous drawable.
    // The bounds callback belongs in the same flush: it re-anchors the
    // viewport translation and (with a baked camera) re-fits the camera
    // overlay to the new canvas. Running it after the flush lands both one
    // render later, so the native still is first composed with the new size
    // but the old overlay and offset, then corrected - a visible jitter.
    // eslint-disable-next-line @eslint-react/dom-no-flush-sync
    flushSync(() => {
      onBoundsChange?.(bounds);
      onChange?.(next);
    });
  };
  const flushPointer = () => {
    frameRef.current = null;
    const pointer = pendingPointerRef.current;
    pendingPointerRef.current = null;
    if (pointer) applyMove(pointer);
  };
  const move = (event: ReactPointerEvent) => {
    if (!activeRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    pendingPointerRef.current = {
      altKey: event.altKey,
      clientX: event.clientX,
      clientY: event.clientY,
    };
    if (frameRef.current === null)
      frameRef.current = requestAnimationFrame(flushPointer);
  };
  const finish = (event: ReactPointerEvent) => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    flushPointer();
    activeRef.current = null;
    onResizeEnd?.(draftRef.current);
    editGesture.endGesture();
    event.stopPropagation();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (
      event.currentTarget instanceof HTMLElement ||
      event.currentTarget instanceof SVGElement
    )
      event.currentTarget.blur();
  };

  return (
    <TransformControls
      frame={{ height: output.height, width: output.width, x: 0, y: 0 }}
      interaction={{
        onPointerCancel: finish,
        onPointerMove: move,
        onPointerUp: finish,
      }}
      lineStyle="solid"
      resize={{
        label: (edges) => `Resize canvas ${edges.join(" ")}`,
        onPointerDown: begin,
      }}
    />
  );
}
