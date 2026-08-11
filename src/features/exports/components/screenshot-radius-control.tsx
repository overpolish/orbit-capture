// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEvent as ReactPointerEvent, RefObject, useRef } from "react";

import {
  clamp,
  RADIUS_HANDLE_INSET,
  RADIUS_HANDLE_TRAVEL,
} from "../camera-overlay-geometry";

export function ScreenshotRadiusControl({
  height,
  mediaRef,
  onChange,
  onChangeEnd,
  radiusPercent,
  width,
}: {
  height: number;
  mediaRef: RefObject<HTMLDivElement | null>;
  radiusPercent: number;
  width: number;
  onChange?: (radiusPercent: number) => void;
  onChangeEnd?: () => void;
}) {
  const activeRef = useRef(false);
  const radius = (Math.min(width, height) * radiusPercent) / 100;
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const offset = `calc(${(radius * RADIUS_HANDLE_TRAVEL).toString()}px + ${RADIUS_HANDLE_INSET.toString()}px * ${inverseScale})`;
  const size = `calc(8px * ${inverseScale})`;

  const move = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!activeRef.current || !bounds || bounds.width === 0) return;
    event.preventDefault();
    event.stopPropagation();
    const scale = bounds.width / width;
    const x = (event.clientX - bounds.left) / scale;
    const y = (event.clientY - bounds.top) / scale;
    const shortest = Math.min(width, height);
    const nextRadius = clamp(
      ((x + y) / 2 - RADIUS_HANDLE_INSET / scale) / RADIUS_HANDLE_TRAVEL,
      0,
      shortest / 2,
    );
    onChange?.((nextRadius * 100) / shortest);
  };

  const finish = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (!activeRef.current) return;
    event.stopPropagation();
    activeRef.current = false;
    onChangeEnd?.();
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    event.currentTarget.blur();
  };

  return (
    <button
      aria-label={`Screenshot corner radius ${Math.round(radiusPercent).toString()} percent`}
      className="absolute rounded-full border-0 bg-white p-0 outline-none"
      onPointerCancel={finish}
      onPointerDown={(event) => {
        event.preventDefault();
        event.stopPropagation();
        activeRef.current = true;
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={move}
      onPointerUp={finish}
      style={{
        cursor: "nwse-resize",
        height: size,
        left: offset,
        top: offset,
        transform: "translate(-50%, -50%)",
        width: size,
      }}
      tabIndex={-1}
      type="button"
    />
  );
}
