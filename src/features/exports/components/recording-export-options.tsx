// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ArrowRight } from "lucide-react";
import { Selection, ToggleButtonGroup } from "react-aria-components";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { formatBytes } from "../duration";
import {
  cameraResolutionScales,
  scaledDimensions,
  scaledVideoDimensions,
} from "../resolution";
import { ExportArtifact } from "../types";

const compressionOptions = [
  { label: "Original", value: 0 },
  { label: "High", value: 1 },
  { label: "Balanced", value: 2 },
  { label: "Smaller", value: 3 },
  { label: "Smallest", value: 4 },
] as const;

const selectedNumber = (selection: Selection) => {
  if (selection === "all") return null;
  const selected = selection.values().next().value;
  return selected === undefined ? null : Number(selected);
};

const formatResolutionRatio = (ratio: number) =>
  ratio.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");

export function RecordingExportOptions({
  artifact,
  availableResolutionScales,
  bakeCamera,
  cameraCompression,
  cameraResolutionScale,
  compression,
  effectiveResolutionScale,
  estimatedSizeBytes,
  isEstimatingSize,
  isSaving,
  onCameraCompressionChange,
  onCameraResolutionScaleChange,
  onCompressionChange,
  onResolutionScaleChange,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  availableResolutionScales: number[];
  bakeCamera: boolean;
  cameraCompression: number;
  cameraResolutionScale: number;
  compression: number;
  effectiveResolutionScale: number;
  estimatedSizeBytes?: number | null;
  isEstimatingSize?: boolean;
  isSaving?: boolean;
  onCameraCompressionChange?: (compression: number) => void;
  onCameraResolutionScaleChange?: (scale: number) => void;
  onCompressionChange?: (compression: number) => void;
  onResolutionScaleChange?: (scale: number) => void;
}) {
  return (
    <div className="flex flex-col gap-2.5">
      <div
        className={
          artifact.camera && !bakeCamera ? "grid grid-cols-2 gap-3" : undefined
        }
      >
        <VideoExportSettings
          compression={compression}
          isDisabled={!artifact.canCompress || isSaving}
          onCompressionChange={onCompressionChange}
          onResolutionScaleChange={onResolutionScaleChange}
          resolutionDimensions={(scale) => scaledDimensions(artifact, scale)}
          resolutionScale={effectiveResolutionScale}
          resolutionScales={availableResolutionScales}
          title={artifact.camera && !bakeCamera ? "Screen" : undefined}
        />
        {artifact.camera && !bakeCamera ? (
          <VideoExportSettings
            compression={cameraCompression}
            isDisabled={!artifact.canCompress || isSaving}
            onCompressionChange={onCameraCompressionChange}
            onResolutionScaleChange={onCameraResolutionScaleChange}
            resolutionDimensions={(scale) =>
              scaledVideoDimensions({
                height: artifact.camera?.height ?? 0,
                scale,
                sourceScale: 100,
                width: artifact.camera?.width ?? 0,
              })
            }
            resolutionScale={cameraResolutionScale}
            resolutionScales={cameraResolutionScales}
            title="Camera"
          />
        ) : null}
      </div>
      <div className="mt-1 flex items-center justify-end gap-1.5 text-xxs text-muted tabular-nums">
        <span>{formatBytes(artifact.originalSizeBytes)} original</span>
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
    </div>
  );
}

function VideoExportSettings({
  compression,
  isDisabled,
  onCompressionChange,
  onResolutionScaleChange,
  resolutionDimensions,
  resolutionScale,
  resolutionScales,
  title,
}: {
  compression: number;
  resolutionDimensions: (scale: number) => { height: number; width: number };
  resolutionScale: number;
  resolutionScales: number[];
  isDisabled?: boolean;
  onCompressionChange?: (compression: number) => void;
  onResolutionScaleChange?: (scale: number) => void;
  title?: string;
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2.5">
      {title ? (
        <h2 className="m-0 text-center text-xs font-medium text-content-fg">
          {title}
        </h2>
      ) : null}
      <div className="flex items-center justify-between gap-2">
        <span className="shrink-0 text-xs text-content-fg">Compression</span>
        <ToggleButtonGroup
          aria-label={title ? `${title} compression` : "Compression"}
          className="flex min-w-0 shrink gap-1"
          disallowEmptySelection
          isDisabled={isDisabled}
          onSelectionChange={(selection) => {
            const selected = selectedNumber(selection);
            if (selected !== null) onCompressionChange?.(selected);
          }}
          selectedKeys={new Set([compression.toString()])}
          selectionMode="single"
        >
          {compressionOptions.map((option) => (
            <ToggleButton
              id={option.value.toString()}
              key={option.value}
              size="sm"
            >
              {option.label}
            </ToggleButton>
          ))}
        </ToggleButtonGroup>
      </div>
      {resolutionScales.length > 1 ? (
        <div className="flex items-center justify-between gap-2">
          <span className="shrink-0 text-xs text-content-fg">Resolution</span>
          <ToggleButtonGroup
            aria-label={title ? `${title} resolution` : "Resolution"}
            className="flex shrink-0 gap-1"
            disallowEmptySelection
            isDisabled={isDisabled}
            onSelectionChange={(selection) => {
              const selected = selectedNumber(selection);
              if (selected !== null) onResolutionScaleChange?.(selected);
            }}
            selectedKeys={new Set([resolutionScale.toString()])}
            selectionMode="single"
          >
            {resolutionScales.map((scale, index) => {
              const dimensions = resolutionDimensions(scale);
              const visibleLabel =
                index === 0
                  ? "Original"
                  : `${formatResolutionRatio(scale / resolutionScales[0])}×`;
              const description = `${visibleLabel}, ${dimensions.width.toString()} by ${dimensions.height.toString()} pixels`;

              return (
                <ToggleButton
                  aria-label={description}
                  id={scale.toString()}
                  key={scale}
                  size="sm"
                >
                  {visibleLabel}
                </ToggleButton>
              );
            })}
          </ToggleButtonGroup>
        </div>
      ) : null}
    </section>
  );
}
