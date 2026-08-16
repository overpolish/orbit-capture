// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEvent as ReactPointerEvent, RefObject, useRef } from "react";

import {
  clamp,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
} from "../camera-overlay-geometry";

export function ScreenshotRadiusControl({
  anchor = "top-left",
  canvasWidth,
  height,
  mediaRef,
  onChange,
  onChangeEnd,
  placementOffset = { x: 0, y: 0 },
  radiusPercent,
  width,
}: {
  height: number;
  mediaRef: RefObject<HTMLDivElement | null>;
  radiusPercent: number;
  width: number;
  anchor?: "top-left" | "top-right";
  canvasWidth?: number;
  onChange?: (radiusPercent: number) => void;
  onChangeEnd?: () => void;
  placementOffset?: { x: number; y: number };
}) {
  const activeRef = useRef(false);
  const radius = (Math.min(width, height) * radiusPercent) / 100;
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const handleOffset = `calc(${(radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;

  const move = (event: ReactPointerEvent<SVGSVGElement>) => {
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!activeRef.current || !bounds || bounds.width === 0) return;
    event.preventDefault();
    event.stopPropagation();
    // This control lives in output-canvas coordinates even when its crop is
    // much larger or smaller than the canvas. Deriving scale from the crop
    // made the handle diverge from the pointer at large screenshot scales.
    const scale = bounds.width / (canvasWidth ?? width);
    const pointerX = (event.clientX - bounds.left) / scale - placementOffset.x;
    const x = anchor === "top-right" ? width - pointerX : pointerX;
    const y = (event.clientY - bounds.top) / scale - placementOffset.y;
    const shortest = Math.min(width, height);
    const nextRadius = clamp(
      ((x + y) / 2 - RADIUS_HANDLE_INSET / scale) / RADIUS_HANDLE_TRAVEL,
      0,
      shortest / 2,
    );
    onChange?.((nextRadius * 100) / shortest);
  };

  const finish = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!activeRef.current) return;
    event.stopPropagation();
    activeRef.current = false;
    onChangeEnd?.();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    event.currentTarget.blur();
  };

  return (
    <svg
      aria-label={`${anchor === "top-right" ? "Background" : "Screenshot"} corner radius ${Math.round(radiusPercent).toString()} percent`}
      className="absolute overflow-visible outline-none"
      height="16"
      onPointerCancel={finish}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.stopPropagation();
        activeRef.current = true;
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={move}
      onPointerUp={finish}
      role="button"
      style={{
        cursor: anchor === "top-right" ? "nesw-resize" : "nwse-resize",
        height: `calc(16px * ${inverseScale})`,
        ...(anchor === "top-right"
          ? { right: handleOffset }
          : {
              left: `calc(${placementOffset.x.toString()}px + ${handleOffset})`,
            }),
        top:
          anchor === "top-right"
            ? handleOffset
            : `calc(${placementOffset.y.toString()}px + ${handleOffset})`,
        transform:
          anchor === "top-right"
            ? "translate(50%, -50%)"
            : "translate(-50%, -50%)",
        transformOrigin: "center",
        width: `calc(16px * ${inverseScale})`,
      }}
      tabIndex={-1}
      viewBox="0 0 16 16"
    >
      <circle className="fill-transparent" cx="8" cy="8" r="8" />
      <circle className="fill-white" cx="8" cy="8" r="4" />
    </svg>
  );
}
