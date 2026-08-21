// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { PointerEvent as ReactPointerEvent } from "react";

import { LABEL_HEIGHT, labelWidth } from "./ruler-label-metrics";
import { LabelHandles } from "./use-label-handles";

/** Every chip wears the app tooltip palette — black and white, no variants. */
export function SvgLabel({
  handles,
  labelKey,
  text,
  x,
  y,
}: {
  text: string;
  x: number;
  y: number;
  handles?: LabelHandles;
  labelKey?: string;
}) {
  const width = labelWidth(text);
  // Pointer events are re-enabled on the chip alone; its overlay stays inert.
  const interaction =
    handles && labelKey
      ? {
          className: "pointer-events-auto cursor-move",
          onPointerCancel: handles.endDrag,
          onPointerDown: (event: ReactPointerEvent<SVGGElement>) => {
            handles.beginDrag(labelKey, event);
          },
          onPointerEnter: () => {
            handles.enter(labelKey);
          },
          onPointerLeave: () => {
            handles.leave(labelKey);
          },
          onPointerMove: handles.drag,
          onPointerUp: handles.endDrag,
        }
      : undefined;
  const offset = handles && labelKey ? handles.offset(labelKey) : undefined;
  const left = x + (offset?.x ?? 0) - width / 2;
  const top = y + (offset?.y ?? 0) - LABEL_HEIGHT / 2;
  return (
    <g
      {...interaction}
      stroke="none"
      transform={`translate(${String(left)} ${String(top)})`}
    >
      <rect
        className="fill-content-fg"
        height={LABEL_HEIGHT}
        rx="4"
        width={width}
      />
      <text
        className="fill-content font-semibold tabular-nums"
        dominantBaseline="central"
        fontSize="10"
        textAnchor="middle"
        x={width / 2}
        y={LABEL_HEIGHT / 2}
      >
        {text}
      </text>
    </g>
  );
}
