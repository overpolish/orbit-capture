// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RulerComponentBox } from "./api";
import { Centerline, centerlines, InnerObject } from "./centerlines";
import { Axis } from "./gradient-field";
import { Bounds } from "./pixel-analysis";

function CenterlineSvg({
  axis,
  bounds,
  line,
}: {
  axis: Axis;
  bounds: Bounds;
  line: Centerline;
}) {
  const vertical = axis === "x";
  return (
    <line
      className="stroke-error"
      opacity={line.accent ? 0.85 : 0.45}
      strokeDasharray="3 3"
      vectorEffect="non-scaling-stroke"
      x1={vertical ? line.position : bounds.x}
      x2={vertical ? line.position : bounds.x + bounds.width}
      y1={vertical ? bounds.y : line.position}
      y2={vertical ? bounds.y + bounds.height : line.position}
    />
  );
}

/** How far a centre tick reaches out of an object's middle, in world px. */
const TICK_LENGTH = 12;

/**
 * One piece of content inside the measurement: a faint outline, plus a green
 * tick through its middle on whichever axis it is centred on.
 */
function InnerObjectSvg({ object }: { object: InnerObject }) {
  const { bounds } = object;
  const centreX = bounds.x + bounds.width / 2;
  const centreY = bounds.y + bounds.height / 2;
  const tickX = Math.min(bounds.height, TICK_LENGTH) / 2;
  const tickY = Math.min(bounds.width, TICK_LENGTH) / 2;
  return (
    <>
      <rect
        className="stroke-content-fg"
        fill="none"
        height={Math.max(1, bounds.height)}
        opacity={0.3}
        strokeDasharray="2 3"
        vectorEffect="non-scaling-stroke"
        width={Math.max(1, bounds.width)}
        x={bounds.x}
        y={bounds.y}
      />
      {object.alignedX ? (
        <line
          className="stroke-error"
          opacity={0.9}
          strokeWidth={2.5}
          vectorEffect="non-scaling-stroke"
          x1={centreX}
          x2={centreX}
          y1={centreY - tickX}
          y2={centreY + tickX}
        />
      ) : null}
      {object.alignedY ? (
        <line
          className="stroke-error"
          opacity={0.9}
          strokeWidth={2.5}
          vectorEffect="non-scaling-stroke"
          x1={centreX - tickY}
          x2={centreX + tickY}
          y1={centreY}
          y2={centreY}
        />
      ) : null}
    </>
  );
}

/**
 * Dashed middles for a measurement, accented when they line up with a sibling
 * measurement or with the content the measurement wraps, over faint outlines
 * of that content with its own centring called out.
 */
export function MeasurementCenterlines({
  bounds,
  boxes,
  deviceScale,
  drawn,
  peers,
}: {
  bounds: Bounds;
  boxes: readonly RulerComponentBox[];
  deviceScale: number;
  peers: readonly Bounds[];
  drawn?: Bounds;
}) {
  // Accents always come from the final bounds so they cannot flicker; only
  // the geometry follows `drawn` while a settle is in flight.
  const lines = centerlines({ bounds, boxes, deviceScale, peers });
  const rect = drawn ?? bounds;
  return (
    <>
      {/* Outlines sit at fixed content positions, so they are held back until
          a settling measurement has landed on them. */}
      {drawn
        ? null
        : lines.objects.map((object) => (
            <InnerObjectSvg
              key={`${String(object.bounds.x)}:${String(object.bounds.y)}:${String(object.bounds.width)}:${String(object.bounds.height)}`}
              object={object}
            />
          ))}
      <CenterlineSvg
        axis="x"
        bounds={rect}
        line={{ ...lines.x, position: rect.x + rect.width / 2 }}
      />
      <CenterlineSvg
        axis="y"
        bounds={rect}
        line={{ ...lines.y, position: rect.y + rect.height / 2 }}
      />
    </>
  );
}
