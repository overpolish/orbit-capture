// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Camera, Image as ImageIcon, Monitor, ZoomIn } from "lucide-react";

import { Badge } from "../../../components/base/badge/badge";
import { NumberField } from "../../../components/base/input-fields/number-field";

export type PreviewBadge = {
  height: number;
  kind: "camera" | "screen" | "screenshot";
  width: number;
};

const iconFor = (kind: PreviewBadge["kind"]) => {
  if (kind === "camera") return <Camera aria-hidden="true" size={12} />;
  if (kind === "screen") return <Monitor aria-hidden="true" size={12} />;
  return <ImageIcon aria-hidden="true" size={12} />;
};

export function PreviewToolbar({
  badges,
  onZoomChange,
  zoomPercent,
}: {
  badges: PreviewBadge[];
  onZoomChange: (zoomPercent: number) => void;
  zoomPercent: number;
}) {
  return (
    <div className="flex h-9 shrink-0 items-center justify-between border-b border-muted/15 px-3 text-muted">
      <div className="flex min-w-0 items-center gap-1.5">
        {badges.map((badge) => {
          return (
            <Badge
              className="shrink-0 font-light tabular-nums"
              key={badge.kind}
              size="xs"
              variant="ghost"
            >
              {iconFor(badge.kind)}
              {badge.width} &times; {badge.height}
            </Badge>
          );
        })}
      </div>
      <NumberField
        aria-label="Preview zoom"
        className="w-24 font-light tabular-nums"
        leftSection={<ZoomIn className="shrink-0 text-muted" size={14} />}
        maxValue={1_600}
        minValue={10}
        onChange={(value) => {
          onZoomChange(Math.round(value));
        }}
        rightAligned
        rightSection={<span className="text-xs text-muted">%</span>}
        scrubbable
        showSteppers={false}
        size="sm"
        step={1}
        value={zoomPercent}
        variant="ghost"
      />
    </div>
  );
}
