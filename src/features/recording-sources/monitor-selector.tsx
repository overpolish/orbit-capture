// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Monitor } from "lucide-react";

import { Button } from "../../components/base/button/button";

import { MonitorDetails } from "./types";

type MonitorSelectorProps = {
  monitors: MonitorDetails[];
  onSelect: (monitor: MonitorDetails) => void;
  selectedMonitor: MonitorDetails | null;
};

export function MonitorSelector({
  monitors,
  onSelect,
  selectedMonitor,
}: MonitorSelectorProps) {
  if (monitors.length === 0) {
    return (
      <div className="flex h-full items-center justify-center text-xs text-muted">
        No displays found
      </div>
    );
  }

  const bounds = monitors.reduce(
    (current, monitor) => ({
      maxX: Math.max(
        current.maxX,
        monitor.layoutPosition.x + monitor.layoutSize.width,
      ),
      maxY: Math.max(
        current.maxY,
        monitor.layoutPosition.y + monitor.layoutSize.height,
      ),
      minX: Math.min(current.minX, monitor.layoutPosition.x),
      minY: Math.min(current.minY, monitor.layoutPosition.y),
    }),
    {
      maxX: Number.NEGATIVE_INFINITY,
      maxY: Number.NEGATIVE_INFINITY,
      minX: Number.POSITIVE_INFINITY,
      minY: Number.POSITIVE_INFINITY,
    },
  );
  const layoutWidth = bounds.maxX - bounds.minX;
  const layoutHeight = bounds.maxY - bounds.minY;

  return (
    <div
      className="relative max-h-full max-w-full"
      style={{
        aspectRatio: layoutWidth / layoutHeight,
        height: `min(84%, 84vw / ${String(layoutWidth / layoutHeight)})`,
        width: `min(88%, 88vh * ${String(layoutWidth / layoutHeight)})`,
      }}
    >
      {monitors.map((monitor) => {
        const isSelected = selectedMonitor?.id === monitor.id;

        return (
          <Button
            aria-label={`Select ${monitor.name}`}
            className="absolute min-h-8 min-w-12 transform-gpu justify-center overflow-hidden px-2 shadow-md"
            color={isSelected ? "info" : "neutral"}
            key={monitor.id}
            onPress={() => {
              onSelect(monitor);
            }}
            showFocus={false}
            style={{
              height: `${String((monitor.layoutSize.height / layoutHeight) * 100)}%`,
              left: `${String(((monitor.layoutPosition.x - bounds.minX) / layoutWidth) * 100)}%`,
              top: `${String(((monitor.layoutPosition.y - bounds.minY) / layoutHeight) * 100)}%`,
              width: `${String((monitor.layoutSize.width / layoutWidth) * 100)}%`,
            }}
            variant="soft"
          >
            <span className="flex min-w-0 flex-col items-center gap-1">
              <Monitor aria-hidden size={14} />
              <span className="max-w-full truncate text-xxs">
                {monitor.name}
              </span>
              {monitor.isPrimary ? (
                <span className="text-[8px] font-medium opacity-70">
                  Primary
                </span>
              ) : null}
            </span>
          </Button>
        );
      })}
    </div>
  );
}
