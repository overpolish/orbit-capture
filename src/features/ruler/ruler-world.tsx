// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CSSProperties } from "react";

import { RulerComponentBox } from "./api";
import { FrozenRulerSnapshot } from "./frozen-ruler-snapshot";
import { Bounds, PixelSnapshot } from "./pixel-analysis";
import { RulerSvgOverlay } from "./ruler-svg-overlay";
import { DistanceProbe, Measurement } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";
import { SelectedLine } from "./use-ruler-deletion";

export function RulerWorld({
  boxes,
  centerlines,
  detectedBoxes,
  deviceScale,
  distanceProbes,
  draft,
  handles,
  highlighted,
  measurements,
  monitorId,
  onLoad,
  style,
}: {
  boxes: readonly RulerComponentBox[];
  centerlines: boolean;
  detectedBoxes: boolean;
  deviceScale: number;
  distanceProbes: readonly DistanceProbe[];
  handles: LabelHandles;
  measurements: readonly Measurement[];
  monitorId: number;
  onLoad: (snapshot: PixelSnapshot) => void;
  style: CSSProperties;
  draft?: Bounds;
  highlighted?: SelectedLine;
}) {
  return (
    <div className="pointer-events-none absolute inset-0" style={style}>
      <FrozenRulerSnapshot monitorId={monitorId} onLoad={onLoad} />
      <RulerSvgOverlay
        boxes={boxes}
        centerlines={centerlines}
        detectedBoxes={detectedBoxes}
        deviceScale={deviceScale}
        distanceProbes={distanceProbes}
        draft={draft}
        handles={handles}
        highlighted={highlighted}
        measurements={measurements}
      />
    </div>
  );
}
