// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Crop } from "lucide-react";
import { MouseEvent as ReactMouseEvent, ReactNode, useState } from "react";
import { TooltipTrigger } from "react-aria-components";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Keyboard } from "../../../components/base/keyboard/keyboard";
import { Tooltip } from "../../../components/base/tooltip/tooltip";
import {
  scaledDimensions,
  scaledVideoDimensions,
  sourceScalePercent,
} from "../resolution";
import {
  resetScreenshotLayout,
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
  ExportArtifact,
  PreparedAudioTrack,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";
import { useExportWindowShortcuts } from "../use-export-window-shortcuts";

import { PreviewToolbar } from "./preview-toolbar";
import { PreviewViewport } from "./preview-viewport";
import { ScrubPreview } from "./scrub-preview";

/**
 * The screenshot section. Sibling to `RecordingSection`, and the reason the
 * frame around them does not know what it is showing.
 */
export function ScreenshotSection({
  artifact,
  onBackgroundRadiusChange,
  onBackgroundRadiusChangeEnd,
  onNeedFullResolution,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  previewUrl,
  radiusPercent,
  screenshotOutput,
}: {
  artifact: ExportArtifact;
  radiusPercent: number;
  onBackgroundRadiusChange?: (radiusPercent: number) => void;
  onBackgroundRadiusChangeEnd?: () => void;
  onNeedFullResolution?: () => void;
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewUrl?: string | null;
  screenshotOutput?: ScreenshotOutputSettings;
}) {
  const [zoomPercent, setZoomPercent] = useState(100);
  const [isEditing, setIsEditing] = useState(false);
  useExportWindowShortcuts({
    onToggleCrop: () => {
      setIsEditing((editing) => !editing);
    },
  });
  const outputDimensions = screenshotOutput
    ? screenshotOutputDimensions(screenshotOutput)
    : { height: artifact.height, width: artifact.width };

  return (
    <div className="flex min-h-0 min-w-0 grow flex-col">
      <PreviewToolbar
        badges={[
          {
            height: outputDimensions.height,
            kind: "screenshot",
            width: outputDimensions.width,
          },
        ]}
        center={
          <TooltipTrigger delay={400}>
            <span
              className="inline-flex"
              onContextMenu={(event: ReactMouseEvent<HTMLSpanElement>) => {
                event.preventDefault();
                if (!screenshotOutput) return;
                onOutputChange?.(
                  resetScreenshotLayout(screenshotOutput, {
                    height: artifact.height,
                    width: artifact.width,
                  }),
                );
              }}
            >
              <ToggleButton
                aria-keyshortcuts="C"
                aria-label="Edit screenshot placement and crop"
                isSelected={isEditing}
                onChange={setIsEditing}
                showFocus={false}
                size="sm"
                variant="ghost"
              >
                <Crop size={15} />
              </ToggleButton>
            </span>
            <Tooltip placement="bottom">
              <span className="flex items-center gap-2">
                Edit placement and crop
                <Keyboard size="xs" variant="tooltip">
                  C
                </Keyboard>
              </span>
            </Tooltip>
          </TooltipTrigger>
        }
        onZoomChange={setZoomPercent}
        zoomPercent={zoomPercent}
      />
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        isEditing={isEditing}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onBackgroundRadiusChange={onBackgroundRadiusChange}
        onBackgroundRadiusChangeEnd={onBackgroundRadiusChangeEnd}
        onNeedFullResolution={onNeedFullResolution}
        onOutputChange={onOutputChange}
        onRadiusChange={onRadiusChange}
        onRadiusChangeEnd={onRadiusChangeEnd}
        onZoomChange={setZoomPercent}
        previewUrl={previewUrl}
        radiusPercent={radiusPercent}
        screenshotOutput={screenshotOutput}
        zoomPercent={zoomPercent}
      />
    </div>
  );
}

/**
 * The recording section: a preview you skim, with what the file is underneath.
 *
 * Framed exactly like the still beside it - no box, no border, just the
 * picture and its shadow - because they are the same kind of thing to the
 * person deciding whether to keep it.
 */
export function RecordingSection({
  artifact,
  audioTrackVolumes,
  bakeCamera,
  cameraOverlay,
  cameraResolutionScalePercent,
  cursorEffects,
  enabledStreamIndices,
  enabledVideoTracks,
  hasCursorData,
  inspector,
  isPreparingRecordingAudio,
  isPreparingRecordingPreview,
  onCameraOverlayChange,
  onEnabledTracksChange,
  onEnabledVideoTracksChange,
  onSelectedTrackChange,
  recordingPreviewError,
  recordingPreviewLayout,
  recordingPreviewTracks,
  resolutionScalePercent,
  selectedTrack,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  audioTrackVolumes?: AudioTrackVolume[];
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  cameraResolutionScalePercent?: number;
  cursorEffects?: CursorEffectSettings;
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
  hasCursorData?: boolean;
  inspector?: ReactNode;
  isPreparingRecordingAudio?: boolean;
  isPreparingRecordingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onEnabledVideoTracksChange?: (tracks: RecordingVideoTrackId[]) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId) => void;
  recordingPreviewError?: string | null;
  recordingPreviewLayout?: RecordingPreviewLayout;
  recordingPreviewTracks?: PreparedAudioTrack[];
  resolutionScalePercent?: number;
  selectedTrack?: RecordingTrackId | null;
}) {
  const primaryOutputDimensions = scaledDimensions(
    artifact,
    resolutionScalePercent ?? sourceScalePercent(artifact),
  );
  const cameraOutputDimensions = artifact.camera
    ? scaledVideoDimensions({
        height: artifact.camera.height,
        scale: cameraResolutionScalePercent ?? 100,
        sourceScale: 100,
        width: artifact.camera.width,
      })
    : undefined;

  return (
    <div className="flex min-h-0 grow flex-col">
      <ScrubPreview
        artifactId={artifact.id}
        audioError={recordingPreviewError}
        audioTracks={recordingPreviewTracks}
        audioTrackVolumes={audioTrackVolumes}
        bakeCamera={bakeCamera}
        cameraOverlay={cameraOverlay}
        cursorEffects={cursorEffects}
        durationMs={artifact.durationMs}
        enabledStreamIndices={enabledStreamIndices}
        enabledVideoTracks={enabledVideoTracks}
        hasCursorData={hasCursorData}
        inspector={inspector}
        isPreparingAudio={isPreparingRecordingAudio}
        isPreparingPreview={isPreparingRecordingPreview}
        key={artifact.id}
        onCameraOverlayChange={onCameraOverlayChange}
        onEnabledTracksChange={onEnabledTracksChange}
        onEnabledVideoTracksChange={onEnabledVideoTracksChange}
        onSelectedTrackChange={onSelectedTrackChange}
        previewLayout={recordingPreviewLayout}
        previewOutputDimensions={{
          primary: primaryOutputDimensions,
          ...(cameraOutputDimensions ? { camera: cameraOutputDimensions } : {}),
        }}
        selectedTrack={selectedTrack}
      />
    </div>
  );
}
