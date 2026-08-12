// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CanvasRect } from "./selection-frame";

function OverlayRegion({
  region,
  selected,
}: {
  region: CanvasRect;
  selected: boolean;
}) {
  return (
    <span
      className={
        selected
          ? "pointer-events-none absolute rounded-[2px] bg-info/45 outline outline-1 outline-info/80"
          : "pointer-events-none absolute rounded-[2px] bg-info/20 outline outline-1 outline-info/65"
      }
      style={{
        height: `${(region.height * 100).toString()}%`,
        left: `${(region.x * 100).toString()}%`,
        top: `${(region.y * 100).toString()}%`,
        width: `${(region.width * 100).toString()}%`,
      }}
    />
  );
}

export function SelectionOverlay({
  regions,
  selectedRegions = [],
}: {
  regions: readonly CanvasRect[];
  selectedRegions?: readonly CanvasRect[];
}) {
  return (
    <>
      {regions.map((region, index) => (
        <OverlayRegion
          key={`region-${index.toString()}`}
          region={region}
          selected={false}
        />
      ))}
      {selectedRegions.map((region, index) => (
        <OverlayRegion
          key={`selected-${index.toString()}`}
          region={region}
          selected
        />
      ))}
    </>
  );
}
