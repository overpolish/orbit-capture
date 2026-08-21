// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { guideGaps } from "./guide-gaps";
import { PixelSize } from "./pixel-analysis";
import { SvgLabel } from "./ruler-svg-label";
import { Guide } from "./ruler-types";
import { LabelHandles } from "./use-label-handles";

const MARGIN = 24;

const clamp = (value: number, limit: number) =>
  Math.min(Math.max(value, MARGIN), Math.max(MARGIN, limit - MARGIN));

/**
 * Gap chips park at the midpoint of the two guides' placement anchors, clamped
 * into view, so they hold still while the pointer moves. Drag one to move it.
 */
export function GuideGapLabels({
  guides,
  handles,
  viewport,
}: {
  guides: readonly Guide[];
  handles: LabelHandles;
  viewport: PixelSize;
}) {
  return guideGaps({ guides, viewport }).map((gap) => {
    const across = clamp(
      gap.anchor,
      gap.axis === "x" ? viewport.height : viewport.width,
    );
    return (
      <SvgLabel
        handles={handles}
        key={gap.key}
        labelKey={gap.key}
        text={`${String(gap.value)} px`}
        x={gap.axis === "x" ? gap.centre : across}
        y={gap.axis === "x" ? across : gap.centre}
      />
    );
  });
}
