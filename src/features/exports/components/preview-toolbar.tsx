// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ZoomIn } from "lucide-react";
import { memo, ReactNode } from "react";

import { NumberField } from "../../../components/base/input-fields/number-field";

import { MAXIMUM_ZOOM } from "./preview-transform";

/**
 * Memoized: the zoom field and the tool buttons are react-aria trees that cost
 * more to re-render than the whole native preview pane, and none of their props
 * change while a canvas-resize gesture updates the output draft at pointer rate.
 */
export const PreviewToolbar = memo(function PreviewToolbar({
  onZoomChange,
  tools,
  zoomPercent,
}: {
  onZoomChange: (zoomPercent: number) => void;
  zoomPercent: number;
  tools?: ReactNode;
}) {
  return (
    <div className="flex h-9 shrink-0 items-center justify-between border-b border-muted/15 px-3 text-muted">
      <div className="flex min-w-0 items-center gap-1">{tools}</div>
      <NumberField
        aria-label="Preview zoom"
        className="w-24 font-light tabular-nums"
        leftSection={<ZoomIn className="shrink-0 text-muted" size={14} />}
        maxValue={MAXIMUM_ZOOM * 100}
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
});
