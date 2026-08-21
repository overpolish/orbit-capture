// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RulerComponentBox } from "./api";
import { Bounds } from "./pixel-analysis";
import { MeasurementCenterlines } from "./ruler-centerlines";
import { LABEL_HEIGHT, labelWidth } from "./ruler-label-metrics";
import { DistanceProbeSvg } from "./ruler-probe-svg";
import { SvgLabel } from "./ruler-svg-label";
import { DistanceProbe, Measurement } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";
import { SelectedLine } from "./use-ruler-deletion";
import { useSettleAnimation } from "./use-settle-animation";

function MeasurementSvg({
  handles,
  measurement,
  selected,
  showLabel = true,
}: {
  measurement: Measurement;
  handles?: LabelHandles;
  selected?: boolean;
  showLabel?: boolean;
}) {
  const horizontal = measurement.height < 8;
  const vertical = measurement.width < 8;
  const width = Math.round(measurement.width);
  const height = Math.round(measurement.height);
  const text = horizontal
    ? `${String(width)} px`
    : vertical
      ? `${String(height)} px`
      : `${String(width)} × ${String(height)} px`;
  const textWidth = labelWidth(text);
  const radius = Math.min(measurement.width, measurement.height) < 24 ? 2 : 4;
  // Inside only when the label leaves the measured content readable: at most
  // half of the box's area, with breathing room left on both axes.
  const fitsInside =
    measurement.width >= textWidth + 16 &&
    measurement.height >= LABEL_HEIGHT + 16 &&
    textWidth * LABEL_HEIGHT <= 0.5 * measurement.width * measurement.height;
  let labelX = measurement.x + measurement.width / 2;
  let labelY = measurement.y + measurement.height / 2;
  if (measurement.id === 0) {
    labelX = measurement.x + measurement.width + 8 + textWidth / 2;
    labelY = measurement.y + measurement.height + 8 + LABEL_HEIGHT / 2;
  } else if (!fitsInside && (horizontal || measurement.y < 24)) {
    labelY = measurement.y + measurement.height + 4 + LABEL_HEIGHT / 2;
  } else if (!fitsInside && vertical) {
    labelX = measurement.x + measurement.width + 4 + textWidth / 2;
  } else if (!fitsInside) {
    labelY = measurement.y - 4 - LABEL_HEIGHT / 2;
  }
  return (
    <g>
      {/* A pulsing halo marks the box the cursor has picked for deletion.
          Always mounted so the opacity transition animates it in AND out. */}
      <rect
        className={
          selected
            ? "animate-halo stroke-error transition-opacity duration-75"
            : "stroke-error transition-opacity duration-75"
        }
        fill="none"
        height={Math.max(1, measurement.height)}
        opacity={selected ? 0.4 : 0}
        rx={radius}
        strokeWidth={7}
        vectorEffect="non-scaling-stroke"
        width={Math.max(1, measurement.width)}
        x={measurement.x}
        y={measurement.y}
      />
      <rect
        className="fill-error/8 stroke-error"
        height={Math.max(1, measurement.height)}
        rx={radius}
        vectorEffect="non-scaling-stroke"
        width={Math.max(1, measurement.width)}
        x={measurement.x}
        y={measurement.y}
      />
      {showLabel ? (
        <SvgLabel
          handles={handles}
          labelKey={`m${String(measurement.id)}`}
          text={text}
          x={labelX}
          y={labelY}
        />
      ) : null}
    </g>
  );
}

function Centerlines({
  boxes,
  deviceScale,
  frames,
  items,
}: {
  boxes: readonly RulerComponentBox[];
  deviceScale: number;
  frames: ReadonlyMap<number, Bounds>;
  items: readonly Measurement[];
}) {
  return items.map((item) => (
    <MeasurementCenterlines
      bounds={item}
      boxes={boxes}
      deviceScale={deviceScale}
      drawn={frames.get(item.id)}
      key={item.id}
      peers={items.filter((other) => other.id !== item.id)}
    />
  ));
}

export function RulerSvgOverlay({
  boxes,
  centerlines,
  detectedBoxes,
  deviceScale,
  distanceProbes,
  draft,
  handles,
  highlighted,
  measurements,
}: {
  boxes: readonly RulerComponentBox[];
  centerlines: boolean;
  detectedBoxes: boolean;
  deviceScale: number;
  distanceProbes: readonly DistanceProbe[];
  handles: LabelHandles;
  measurements: readonly Measurement[];
  draft?: Bounds;
  highlighted?: SelectedLine;
}) {
  const frames = useSettleAnimation(measurements);
  const settled = measurements.map((measurement) => {
    const frame = frames.get(measurement.id);
    return frame ? { ...measurement, ...frame } : measurement;
  });
  const centred = draft ? [...measurements, { ...draft, id: 0 }] : measurements;
  return (
    <svg className="pointer-events-none absolute inset-0 size-full overflow-visible">
      {/* Debug view (KeyB): every box the detector found at this tolerance,
          converted from device px to world coordinates. */}
      {detectedBoxes
        ? boxes.map((box) => (
            <rect
              className="stroke-info"
              fill="none"
              height={box.height / deviceScale}
              key={`${String(box.x)}:${String(box.y)}:${String(box.width)}:${String(box.height)}`}
              opacity={0.5}
              strokeDasharray="4 3"
              vectorEffect="non-scaling-stroke"
              width={box.width / deviceScale}
              x={box.x / deviceScale}
              y={box.y / deviceScale}
            />
          ))
        : null}
      {/* Centerlines paint before everything that carries labels, so labels
          always read on top of them. */}
      {centerlines ? (
        <Centerlines
          boxes={boxes}
          deviceScale={deviceScale}
          frames={frames}
          items={centred}
        />
      ) : null}
      {distanceProbes.map((probe) => (
        <DistanceProbeSvg
          handles={handles}
          key={probe.id}
          probe={probe}
          selected={
            highlighted?.kind === "probe" && probe.id === highlighted.id
          }
          showLabel
        />
      ))}
      {settled.map((measurement) => (
        <MeasurementSvg
          handles={handles}
          key={measurement.id}
          measurement={measurement}
          selected={
            highlighted?.kind === "measurement" &&
            measurement.id === highlighted.id
          }
        />
      ))}
      {draft ? (
        <MeasurementSvg measurement={{ ...draft, id: 0 }} showLabel={false} />
      ) : null}
    </svg>
  );
}
