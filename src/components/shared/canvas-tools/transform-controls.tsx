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

function ControlHandle({
  ariaLabel,
  cursor,
  inverseScale,
  left,
  onPointerDown,
  pointerHandlers,
  top,
}: {
  ariaLabel: string;
  cursor: string;
  inverseScale: string;
  left: string;
  onPointerDown: PointerEventHandler<SVGSVGElement>;
  pointerHandlers: PointerHandlers;
  top: string;
}) {
  return (
    <svg
      aria-label={ariaLabel}
      className="pointer-events-auto absolute overflow-visible outline-none"
      height="16"
      onPointerDown={onPointerDown}
      role="button"
      style={{
        cursor,
        height: `calc(16px * ${inverseScale})`,
        left,
        top,
        transform: "translate(-50%, -50%)",
        transformOrigin: "center",
        width: `calc(16px * ${inverseScale})`,
      }}
      tabIndex={-1}
      viewBox="0 0 16 16"
      {...pointerHandlers}
    >
      <circle className="fill-transparent" cx="8" cy="8" r="8" />
      <circle className="fill-white" cx="8" cy="8" r="4" />
    </svg>
  );
}

export function TransformControls({
  frame,
  interaction,
  inverseScale = "var(--preview-inverse-scale, 1)",
  lineStyle = "dashed",
  move,
  radius,
  radiusHandle,
  resize,
  scaleRing,
}: {
  frame: { height: number; width: number; x: number; y: number };
  interaction?: PointerHandlers;
  inverseScale?: string;
  lineStyle?: "dashed" | "solid";
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
    ) => PointerEventHandler<SVGSVGElement>;
  };
  scaleRing?: {
    cursor: string;
    extent: number;
    label: string;
    onPointerDown: PointerEventHandler<SVGCircleElement>;
    onPointerMove?: PointerEventHandler<SVGCircleElement>;
  };
}) {
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
          strokeDasharray={
            lineStyle === "dashed" ? `calc(5px * ${inverseScale})` : undefined
          }
          style={{ strokeWidth: `calc(1px * ${inverseScale})` }}
          width="100%"
        />
      </svg>
      {resize
        ? transformHandles.map(({ edges, x, y }) => (
            <ControlHandle
              ariaLabel={resize.label(edges)}
              cursor={cursorForTransformEdges(edges)}
              inverseScale={inverseScale}
              key={edges.join("-")}
              left={`${String(x * 100)}%`}
              onPointerDown={resize.onPointerDown(edges)}
              pointerHandlers={pointerHandlers}
              top={`${String(y * 100)}%`}
            />
          ))
        : null}
      {move ? (
        <ControlHandle
          ariaLabel={move.label}
          cursor="move"
          inverseScale={inverseScale}
          left="50%"
          onPointerDown={move.onPointerDown}
          pointerHandlers={pointerHandlers}
          top="50%"
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
            style={{ strokeWidth: `calc(2px * ${inverseScale})` }}
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
        <ControlHandle
          ariaLabel={radiusHandle.label}
          cursor={radiusHandle.cursor}
          inverseScale={inverseScale}
          left={radiusHandle.left}
          onPointerDown={radiusHandle.onPointerDown}
          pointerHandlers={pointerHandlers}
          top={radiusHandle.top}
        />
      ) : null}
    </div>
  );
}
