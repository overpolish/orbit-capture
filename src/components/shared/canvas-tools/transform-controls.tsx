// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEventHandler } from "react";

import {
  cursorForTransformEdges,
  TransformEdge,
  transformHandles,
} from "./transform-handles";

type PointerHandlers = {
  onPointerCancel?: PointerEventHandler;
  onPointerMove?: PointerEventHandler;
  onPointerUp?: PointerEventHandler;
};

export function TransformControls({
  frame,
  interaction,
  inverseScale = "var(--preview-inverse-scale, 1)",
  move,
  radius,
  radiusHandle,
  resize,
  scaleRing,
}: {
  frame: { height: number; width: number; x: number; y: number };
  interaction?: PointerHandlers;
  inverseScale?: string;
  move?: { label: string; onPointerDown: PointerEventHandler };
  radius?: number;
  radiusHandle?: {
    cursor: string;
    label: string;
    left: string;
    onPointerDown: PointerEventHandler;
    top: string;
  };
  resize?: {
    label: (edges: TransformEdge[]) => string;
    onPointerDown: (
      edges: TransformEdge[],
    ) => PointerEventHandler<HTMLButtonElement>;
  };
  scaleRing?: {
    cursor: string;
    extent: number;
    label: string;
    onPointerDown: PointerEventHandler<SVGCircleElement>;
    onPointerMove?: PointerEventHandler<SVGCircleElement>;
  };
}) {
  const controlSize = `calc(8px * ${inverseScale})`;
  const lineWidth = `calc(2px * ${inverseScale})`;
  const pointerHandlers = interaction ?? {};

  return (
    <div
      className="pointer-events-none absolute touch-none select-none"
      style={{
        height: frame.height,
        left: frame.x,
        top: frame.y,
        width: frame.width,
      }}
    >
      <svg
        aria-hidden
        className="pointer-events-none absolute inset-0 size-full overflow-visible"
      >
        <rect
          fill="none"
          height="100%"
          rx={radius}
          stroke="white"
          strokeDasharray={`calc(5px * ${inverseScale})`}
          strokeWidth={lineWidth}
          width="100%"
        />
      </svg>
      {resize
        ? transformHandles.map(({ edges, x, y }) => (
            <button
              aria-label={resize.label(edges)}
              className="pointer-events-auto absolute rounded-full border-0 bg-white p-0 outline-none"
              key={edges.join("-")}
              onPointerDown={resize.onPointerDown(edges)}
              style={{
                cursor: cursorForTransformEdges(edges),
                height: controlSize,
                left: `${String(x * 100)}%`,
                top: `${String(y * 100)}%`,
                transform: "translate(-50%, -50%)",
                width: controlSize,
              }}
              tabIndex={-1}
              type="button"
              {...pointerHandlers}
            />
          ))
        : null}
      {move ? (
        <button
          aria-label={move.label}
          className="pointer-events-auto absolute rounded-full border-0 bg-white p-0 outline-none"
          onPointerDown={move.onPointerDown}
          style={{
            height: controlSize,
            left: "50%",
            top: "50%",
            transform: "translate(-50%, -50%)",
            width: controlSize,
          }}
          tabIndex={-1}
          type="button"
          {...pointerHandlers}
        />
      ) : null}
      {scaleRing ? (
        <svg
          className="pointer-events-none absolute overflow-visible"
          style={{
            height: scaleRing.extent * 2,
            left: "50%",
            top: "50%",
            transform: "translate(-50%, -50%)",
            width: scaleRing.extent * 2,
          }}
          viewBox={`0 0 ${String(scaleRing.extent * 2)} ${String(scaleRing.extent * 2)}`}
        >
          <circle
            aria-hidden
            className="fill-none stroke-white"
            cx={scaleRing.extent}
            cy={scaleRing.extent}
            r={Math.max(1, scaleRing.extent - 1)}
            style={{ strokeWidth: lineWidth }}
          />
          <circle
            aria-label={scaleRing.label}
            className="pointer-events-auto fill-none stroke-transparent outline-none select-none"
            cx={scaleRing.extent}
            cy={scaleRing.extent}
            onPointerCancel={interaction?.onPointerCancel}
            onPointerDown={scaleRing.onPointerDown}
            onPointerMove={
              scaleRing.onPointerMove ?? interaction?.onPointerMove
            }
            onPointerUp={interaction?.onPointerUp}
            r={Math.max(1, scaleRing.extent - 1)}
            role="button"
            style={{
              cursor: scaleRing.cursor,
              pointerEvents: "stroke",
              strokeWidth: `calc(10px * ${inverseScale})`,
            }}
            tabIndex={-1}
          />
        </svg>
      ) : null}
      {radiusHandle ? (
        <button
          aria-label={radiusHandle.label}
          className="pointer-events-auto absolute rounded-full border-0 bg-white p-0 outline-none"
          onPointerDown={radiusHandle.onPointerDown}
          style={{
            cursor: radiusHandle.cursor,
            height: controlSize,
            left: radiusHandle.left,
            top: radiusHandle.top,
            transform: "translate(-50%, -50%)",
            width: controlSize,
          }}
          tabIndex={-1}
          type="button"
          {...pointerHandlers}
        />
      ) : null}
    </div>
  );
}
