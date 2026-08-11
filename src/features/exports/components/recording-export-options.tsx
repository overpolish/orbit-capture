// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ArrowRight } from "lucide-react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { PillGroup } from "../../../components/base/pill-group/pill-group";
import { formatBytes } from "../duration";

const compressionOptions = [
  { label: "Original", value: 0 },
  { label: "High", value: 1 },
  { label: "Balanced", value: 2 },
  { label: "Smaller", value: 3 },
  { label: "Smallest", value: 4 },
] as const;

const formatResolutionRatio = (ratio: number) =>
  ratio.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");

export function RecordingSizeEstimate({
  estimatedSizeBytes,
  isEstimatingSize,
  originalSizeBytes,
}: {
  originalSizeBytes: number;
  estimatedSizeBytes?: number | null;
  isEstimatingSize?: boolean;
}) {
  return (
    <div className="flex items-center justify-end gap-1.5 text-xxs text-muted tabular-nums">
      <span>{formatBytes(originalSizeBytes)} original</span>
      <ArrowRight aria-hidden="true" className="shrink-0" size={12} />
      {isEstimatingSize ? (
        <span className="flex items-center gap-1.5">
          <CircularProgressBar
            aria-label="Estimating compressed size"
            isIndeterminate
            size={12}
            strokeWidth={10}
          />
          Estimating
        </span>
      ) : estimatedSizeBytes ? (
        <span>~{formatBytes(estimatedSizeBytes)} estimated</span>
      ) : (
        <span>Estimate unavailable</span>
      )}
    </div>
  );
}

export function VideoExportSettings({
  compression,
  isDisabled,
  onCompressionChange,
  onResolutionScaleChange,
  resolutionDimensions,
  resolutionScale,
  resolutionScales,
}: {
  compression: number;
  resolutionDimensions: (scale: number) => { height: number; width: number };
  resolutionScale: number;
  resolutionScales: number[];
  isDisabled?: boolean;
  onCompressionChange?: (compression: number) => void;
  onResolutionScaleChange?: (scale: number) => void;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2.5">
      <div className="flex flex-col gap-1.5">
        <span className="shrink-0 text-xs text-content-fg">Compression</span>
        <PillGroup
          ariaLabel="Compression"
          className="grid grid-cols-3"
          display="label"
          isDisabled={isDisabled}
          items={compressionOptions.map((option) => ({
            id: option.value.toString(),
            label: option.label,
          }))}
          onSelectionChange={(selected) => {
            onCompressionChange?.(Number(selected));
          }}
          selected={compression.toString()}
        />
      </div>
      {resolutionScales.length > 1 ? (
        <div className="flex flex-col gap-1.5">
          <span className="shrink-0 text-xs text-content-fg">Resolution</span>
          <PillGroup
            ariaLabel="Resolution"
            display="label"
            isDisabled={isDisabled}
            items={resolutionScales.map((scale, index) => {
              const dimensions = resolutionDimensions(scale);
              const label =
                index === 0
                  ? "Original"
                  : `${formatResolutionRatio(scale / resolutionScales[0])}×`;
              return {
                ariaLabel: `${label}, ${dimensions.width.toString()} by ${dimensions.height.toString()} pixels`,
                id: scale.toString(),
                label,
              };
            })}
            onSelectionChange={(selected) => {
              onResolutionScaleChange?.(Number(selected));
            }}
            selected={resolutionScale.toString()}
          />
        </div>
      ) : null}
    </section>
  );
}
