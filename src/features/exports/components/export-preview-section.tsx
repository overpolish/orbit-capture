// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ReactNode, useState } from "react";

import {
  scaledDimensions,
  scaledVideoDimensions,
  sourceScalePercent,
} from "../resolution";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  ExportArtifact,
  PreparedAudioTrack,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import { PreviewToolbar } from "./preview-toolbar";
import { PreviewViewport } from "./preview-viewport";
import { ScrubPreview } from "./scrub-preview";

/**
 * The screenshot section. Sibling to `RecordingSection`, and the reason the
 * frame around them does not know what it is showing.
 */
export function ScreenshotSection({
  artifact,
  onNeedFullResolution,
  onRadiusChange,
  onRadiusChangeEnd,
  previewUrl,
  radiusPercent,
}: {
  artifact: ExportArtifact;
  radiusPercent: number;
  onNeedFullResolution?: () => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewUrl?: string | null;
}) {
  const [zoomPercent, setZoomPercent] = useState(100);

  return (
    <div className="flex min-h-0 min-w-0 grow flex-col">
      <PreviewToolbar
        badges={[
          {
            height: artifact.height,
            kind: "screenshot",
            width: artifact.width,
          },
        ]}
        onZoomChange={setZoomPercent}
        zoomPercent={zoomPercent}
      />
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onNeedFullResolution={onNeedFullResolution}
        onRadiusChange={onRadiusChange}
        onRadiusChangeEnd={onRadiusChangeEnd}
        onZoomChange={setZoomPercent}
        previewUrl={previewUrl}
        radiusPercent={radiusPercent}
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
  enabledStreamIndices,
  enabledVideoTracks,
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
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
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
        durationMs={artifact.durationMs}
        enabledStreamIndices={enabledStreamIndices}
        enabledVideoTracks={enabledVideoTracks}
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
