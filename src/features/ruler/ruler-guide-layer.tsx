// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CSSProperties } from "react";

import { PixelSize } from "./pixel-analysis";
import { GuideGapLabels } from "./ruler-guide-gaps";
import { Guide } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";

function GuideSvg({ guide, selected }: { guide: Guide; selected?: boolean }) {
  const vertical = guide.axis === "x";
  const ends = {
    x1: vertical ? guide.position : 0,
    x2: vertical ? guide.position : "100%",
    y1: vertical ? 0 : guide.position,
    y2: vertical ? "100%" : guide.position,
  } as const;
  return (
    <>
      {/* A pulsing halo marks the guide the cursor has picked for deletion.
          Always mounted so the opacity transition animates it in AND out. */}
      <line
        {...ends}
        className={
          selected
            ? "animate-halo stroke-info transition-opacity duration-75"
            : "stroke-info transition-opacity duration-75"
        }
        opacity={selected ? 0.4 : 0}
        strokeWidth={7}
        vectorEffect="non-scaling-stroke"
      />
      <line
        {...ends}
        className="stroke-info"
        opacity={guide.transient ? 0.7 : 1}
        strokeDasharray={guide.transient ? "6 4" : undefined}
        vectorEffect="non-scaling-stroke"
      />
    </>
  );
}

/**
 * Guides live in their own transformed layer so they paint above the crosshair
 * while the info chips that follow the cursor still sit on top of everything.
 */
export function GuideLayer({
  guides,
  handles,
  preview,
  selectedId,
  style,
  viewport,
}: {
  guides: readonly Guide[];
  handles: LabelHandles;
  style: CSSProperties;
  viewport: PixelSize;
  preview?: Guide;
  selectedId?: number;
}) {
  return (
    <div className="pointer-events-none absolute inset-0" style={style}>
      <svg className="pointer-events-none absolute inset-0 size-full overflow-visible">
        {guides.map((guide) => (
          <GuideSvg
            guide={guide}
            key={guide.id}
            selected={guide.id === selectedId}
          />
        ))}
        {preview ? <GuideSvg guide={preview} /> : null}
        <GuideGapLabels guides={guides} handles={handles} viewport={viewport} />
      </svg>
    </div>
  );
}
