// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useState } from "react";

import { cn } from "../../lib/styling";
import { Region } from "../recording-sources/types";

import { drawnRegion } from "./region-geometry";

type Drawing = {
  previous: Region;
  start: { x: number; y: number };
};

export function RegionDrawingSurface({
  aspect,
  bounds,
  current,
  isEditing,
  onChange,
  onDrawingChange,
  onFinish,
}: {
  bounds: { height: number; width: number };
  current: Region;
  isEditing: boolean;
  onChange: (region: Region) => void;
  onDrawingChange: (drawing: boolean) => void;
  onFinish: (region: Region) => void;
  /** Width over height the drawn region is held to, when one is locked. */
  aspect?: number;
}) {
  const [drawing, setDrawing] = useState<Drawing>();

  const endDrawing = () => {
    setDrawing(undefined);
    onDrawingChange(false);
  };

  return (
    <div
      aria-hidden="true"
      className={cn(
        "absolute inset-0 cursor-crosshair touch-none",
        !isEditing && "pointer-events-none",
      )}
      onPointerCancel={() => {
        if (!drawing) return;
        onChange(drawing.previous);
        endDrawing();
      }}
      onPointerDown={(event) => {
        if (!isEditing || event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        setDrawing({
          previous: current,
          start: { x: event.clientX, y: event.clientY },
        });
        onDrawingChange(true);
      }}
      onPointerMove={(event) => {
        if (!drawing) return;
        onChange(
          drawnRegion({
            aspect,
            bounds,
            end: { x: event.clientX, y: event.clientY },
            start: drawing.start,
          }),
        );
      }}
      onPointerUp={(event) => {
        if (!drawing) return;
        const next = drawnRegion({
          aspect,
          bounds,
          end: { x: event.clientX, y: event.clientY },
          start: drawing.start,
        });
        if (next.size.width <= 1 || next.size.height <= 1) {
          onChange(drawing.previous);
        } else {
          onChange(next);
          onFinish(next);
        }
        endDrawing();
      }}
    />
  );
}
