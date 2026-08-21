// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { LABEL_HEIGHT, labelWidth } from "./ruler-label-metrics";
import { SvgLabel } from "./ruler-svg-label";
import { DistanceProbe } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";

export function DistanceProbeSvg({
  gapAt,
  gapSize = 0,
  handles,
  probe,
  selected,
  showLabel,
}: {
  probe: DistanceProbe;
  showLabel: boolean;
  /** Along-axis centre of the sibling ruler crossing this one. */
  gapAt?: number;
  /** World width of the sibling's stroke — the exact exclusion to leave. */
  gapSize?: number;
  handles?: LabelHandles;
  selected?: boolean;
}) {
  const start = Math.min(probe.start, probe.end);
  const end = Math.max(probe.start, probe.end);
  const distance = Math.round(end - start);
  const text = `${String(distance)} px`;
  const vertical = probe.axis === "y";
  const fits = vertical ? distance >= 28 : distance >= labelWidth(text);
  const center = (start + end) / 2;
  const textWidth = labelWidth(text);
  const labelX = vertical
    ? fits
      ? probe.position
      : probe.position + 8 + textWidth / 2
    : fits
      ? center
      : end + 4 + textWidth / 2;
  const labelY = vertical
    ? fits
      ? center
      : end + 4 + LABEL_HEIGHT / 2
    : probe.position;
  const line = vertical
    ? { x1: probe.position, x2: probe.position, y1: start, y2: end }
    : { x1: start, x2: end, y1: probe.position, y2: probe.position };
  // Each transient ruler stops at the edge of its sibling's stroke, so the
  // intersection excludes exactly their overlap; stamped probes render
  // unbroken.
  const spans: [number, number][] =
    gapAt === undefined || gapSize <= 0
      ? [[start, end]]
      : [
          [start, Math.max(start, gapAt - gapSize / 2)],
          [Math.min(end, gapAt + gapSize / 2), end],
        ];
  const segments = spans
    .filter(([from, to]) => to > from)
    .map(([from, to]) =>
      vertical
        ? { x1: probe.position, x2: probe.position, y1: from, y2: to }
        : { x1: from, x2: to, y1: probe.position, y2: probe.position },
    );
  return (
    <g className="stroke-error">
      {/* A pulsing halo marks the probe the cursor has picked for deletion.
          Always mounted so the opacity transition animates it in AND out. */}
      <line
        {...line}
        className={
          selected
            ? "animate-halo transition-opacity duration-75"
            : "transition-opacity duration-75"
        }
        opacity={selected ? 0.4 : 0}
        strokeWidth={7}
        vectorEffect="non-scaling-stroke"
      />
      {segments.map((segment) => (
        <line
          key={`${String(segment.x1)}:${String(segment.y1)}`}
          {...segment}
          vectorEffect="non-scaling-stroke"
        />
      ))}
      <line
        vectorEffect="non-scaling-stroke"
        x1={vertical ? probe.position - 4 : start}
        x2={vertical ? probe.position + 4 : start}
        y1={vertical ? start : probe.position - 4}
        y2={vertical ? start : probe.position + 4}
      />
      <line
        vectorEffect="non-scaling-stroke"
        x1={vertical ? probe.position - 4 : end}
        x2={vertical ? probe.position + 4 : end}
        y1={vertical ? end : probe.position - 4}
        y2={vertical ? end : probe.position + 4}
      />
      {showLabel ? (
        <SvgLabel
          handles={handles}
          labelKey={probe.id === undefined ? undefined : `p${String(probe.id)}`}
          text={text}
          x={labelX}
          y={labelY}
        />
      ) : null}
    </g>
  );
}
