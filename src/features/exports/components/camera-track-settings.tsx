// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import {
  cameraResolutionScales,
  scaledDimensions,
  scaledVideoDimensions,
} from "../resolution";
import {
  RecordingOutputSettings,
  resizeScreenshotOutputCentered,
  ScreenshotOutputSettings,
} from "../screenshot-output";
import { ExportArtifact } from "../types";

import { VideoExportSettings } from "./recording-export-options";
import { ScreenshotOutputControls } from "./screenshot-inspector";

type RecordingArtifact = Extract<ExportArtifact, { kind: "recording" }>;

export function CameraTrackSettings({
  artifact,
  availableResolutionScales,
  baked,
  cameraCompression,
  cameraResolutionScalePercent,
  compression,
  effectiveResolutionScale,
  isSaving,
  onCameraCompressionChange,
  onCameraResolutionScaleChange,
  onCompressionChange,
  onRecordingOutputChange,
  onResolutionScaleChange,
  recordingOutput,
  resizePrimaryOutput,
}: {
  artifact: RecordingArtifact;
  availableResolutionScales: number[];
  baked: boolean;
  cameraCompression: number;
  cameraResolutionScalePercent: number;
  compression: number;
  effectiveResolutionScale: number;
  resizePrimaryOutput: (width: number, height: number) => void;
  isSaving?: boolean;
  onCameraCompressionChange?: (compression: number) => void;
  onCameraResolutionScaleChange?: (scale: number) => void;
  onCompressionChange?: (compression: number) => void;
  onRecordingOutputChange?: (
    trackId: "camera" | "primary",
    settings: ScreenshotOutputSettings,
  ) => void;
  onResolutionScaleChange?: (scale: number) => void;
  recordingOutput?: RecordingOutputSettings;
}) {
  const camera = artifact.camera;
  if (!camera) return null;
  const output = baked ? recordingOutput?.primary : recordingOutput?.camera;
  const source = baked
    ? { height: artifact.height, width: artifact.width }
    : camera;

  return (
    <div className="flex flex-col gap-4">
      <VideoExportSettings
        compression={baked ? compression : cameraCompression}
        isDisabled={!artifact.canCompress || isSaving}
        onCompressionChange={
          baked ? onCompressionChange : onCameraCompressionChange
        }
        onResolutionScaleChange={
          baked ? onResolutionScaleChange : onCameraResolutionScaleChange
        }
        resolutionDimensions={(scale) =>
          baked
            ? scaledDimensions(artifact, scale)
            : scaledVideoDimensions({
                height: camera.height,
                scale,
                sourceScale: 100,
                width: camera.width,
              })
        }
        resolutionScale={
          baked ? effectiveResolutionScale : cameraResolutionScalePercent
        }
        resolutionScales={
          recordingOutput
            ? []
            : baked
              ? availableResolutionScales
              : cameraResolutionScales
        }
      />
      {output ? (
        <ScreenshotOutputControls
          className=""
          isSaving={isSaving}
          onChange={(settings) => {
            onRecordingOutputChange?.(baked ? "primary" : "camera", settings);
          }}
          onDimensionsChange={
            baked
              ? resizePrimaryOutput
              : (width, height) => {
                  onRecordingOutputChange?.(
                    "camera",
                    resizeScreenshotOutputCentered({
                      height,
                      settings: output,
                      source,
                      width,
                    }),
                  );
                }
          }
          settings={output}
          showDropShadow={!baked}
          sourceHeight={source.height}
          sourceWidth={source.width}
        />
      ) : null}
      {baked && recordingOutput ? (
        <Checkbox
          isDisabled={isSaving}
          isSelected={recordingOutput.camera.dropShadow}
          onChange={(dropShadow) => {
            onRecordingOutputChange?.("camera", {
              ...recordingOutput.camera,
              dropShadow,
            });
          }}
          size="sm"
        >
          <span className="text-xs">Drop shadow</span>
        </Checkbox>
      ) : null}
    </div>
  );
}
