// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  CameraOverlaySettings,
  ExportArtifact,
  PreparedAudioTrack,
  RecordingPreviewLayout,
} from "../types";

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
  return (
    <div className="flex flex-col gap-2">
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onNeedFullResolution={onNeedFullResolution}
        onRadiusChange={onRadiusChange}
        onRadiusChangeEnd={onRadiusChangeEnd}
        previewUrl={previewUrl}
        radiusPercent={radiusPercent}
      />
      <p className="m-0 text-center text-xxs text-muted tabular-nums">
        {artifact.width} &times; {artifact.height}
      </p>
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
  bakeCamera,
  cameraOverlay,
  enabledStreamIndices,
  isPreparingRecordingAudio,
  isPreparingRecordingPreview,
  onCameraOverlayChange,
  onEnabledTracksChange,
  recordingPreviewError,
  recordingPreviewLayout,
  recordingPreviewTracks,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  enabledStreamIndices?: number[];
  isPreparingRecordingAudio?: boolean;
  isPreparingRecordingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  recordingPreviewError?: string | null;
  recordingPreviewLayout?: RecordingPreviewLayout;
  recordingPreviewTracks?: PreparedAudioTrack[];
}) {
  return (
    <div className="flex flex-col gap-2">
      <ScrubPreview
        artifactId={artifact.id}
        audioError={recordingPreviewError}
        audioTracks={recordingPreviewTracks}
        bakeCamera={bakeCamera}
        cameraOverlay={cameraOverlay}
        durationMs={artifact.durationMs}
        enabledStreamIndices={enabledStreamIndices}
        isPreparingAudio={isPreparingRecordingAudio}
        isPreparingPreview={isPreparingRecordingPreview}
        key={artifact.id}
        onCameraOverlayChange={onCameraOverlayChange}
        onEnabledTracksChange={onEnabledTracksChange}
        previewLayout={recordingPreviewLayout}
      />
    </div>
  );
}
