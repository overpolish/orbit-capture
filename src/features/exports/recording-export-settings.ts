// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { CameraOverlaySettings, ExportArtifact } from "./types";

export const defaultCameraOverlay = (
  artifact?: ExportArtifact | null,
): CameraOverlaySettings => {
  const recording = artifact?.kind === "recording" ? artifact : null;
  const camera = recording?.camera;
  const screenWidth = recording?.width || 16;
  const screenHeight = recording?.height || 9;
  const cameraWidth = camera?.width || 16;
  const cameraHeight = camera?.height || 9;
  const requestedWidthPercent = 25;
  const requestedHeightPercent =
    ((screenWidth * requestedWidthPercent) / 100) *
    (cameraHeight / cameraWidth) *
    (100 / screenHeight);
  const frameHeightPercent = Math.min(80, requestedHeightPercent);
  const frameWidthPercent =
    requestedWidthPercent * (frameHeightPercent / requestedHeightPercent);
  const frameXPercent =
    ((screenWidth - (screenWidth * frameWidthPercent) / 100) * 0.96 * 100) /
    screenWidth;
  const frameYPercent =
    ((screenHeight - (screenHeight * frameHeightPercent) / 100) * 0.04 * 100) /
    screenHeight;

  return {
    cameraWidthPercent: frameWidthPercent,
    cameraXPercent: frameXPercent + frameWidthPercent / 2,
    cameraYPercent: frameYPercent + frameHeightPercent / 2,
    frameHeightPercent,
    frameWidthPercent,
    frameXPercent,
    frameYPercent,
    radiusPercent: 8,
  };
};

export type VideoExportSettings = {
  compression: number;
  resolutionScalePercent: number;
};

type RecordingSavePlanOptions = {
  bakeCamera: boolean;
  cameraCompression: number;
  cameraOverlay: CameraOverlaySettings;
  cameraResolutionScalePercent: number;
  collapseAudio: boolean;
  compression: number;
  enabledStreamIndices: number[];
  resolutionScalePercent: number;
};

export type RecordingSavePlan = {
  options: RecordingSavePlanOptions;
  showsMeasuredProgress: boolean;
};

/** The name of a combination of tracks, matching what backend mixes use. */
export const mixSignature = (streamIndices: number[]) =>
  streamIndices.length > 0
    ? [...streamIndices].sort((a, b) => a - b).join("-")
    : "silent";

/** The neutral settings make a missing camera a no-op at every API boundary. */
export const cameraExportSettings = (
  artifact: ExportArtifact | null,
  compression: number,
  resolutionScalePercent: number,
): VideoExportSettings =>
  artifact?.kind === "recording" && artifact.camera
    ? { compression, resolutionScalePercent }
    : { compression: 0, resolutionScalePercent: 100 };

export const recordingSavePlan = ({
  artifact,
  bakeCamera,
  camera,
  cameraOverlay,
  collapseAudio,
  compression,
  enabledStreamIndices,
  originalResolutionScale,
  resolutionScalePercent,
}: {
  artifact: ExportArtifact | null;
  bakeCamera: boolean;
  camera: VideoExportSettings;
  cameraOverlay: CameraOverlaySettings;
  collapseAudio: boolean;
  compression: number;
  enabledStreamIndices: number[] | null;
  originalResolutionScale: number;
  resolutionScalePercent: number;
}): RecordingSavePlan => {
  const selectedIndices = enabledStreamIndices ?? [];
  const hasCamera = artifact?.kind === "recording" && artifact.camera !== null;
  const hasAudioChanges =
    artifact?.kind === "recording" &&
    (selectedIndices.length !== artifact.audioTracks.length ||
      (collapseAudio && selectedIndices.length > 1));
  const hasMeasuredWork =
    artifact?.kind === "recording" &&
    artifact.durationMs > 0 &&
    (artifact.primaryKind === "audio" ||
      compression > 0 ||
      resolutionScalePercent < originalResolutionScale ||
      (hasCamera &&
        (bakeCamera ||
          camera.compression > 0 ||
          camera.resolutionScalePercent < 100)) ||
      hasAudioChanges);

  return {
    options: {
      bakeCamera: hasCamera && bakeCamera,
      cameraCompression: camera.compression,
      cameraOverlay,
      cameraResolutionScalePercent: camera.resolutionScalePercent,
      collapseAudio: collapseAudio && selectedIndices.length > 1,
      compression,
      enabledStreamIndices: selectedIndices,
      resolutionScalePercent,
    },
    showsMeasuredProgress: hasMeasuredWork || hasCamera,
  };
};
