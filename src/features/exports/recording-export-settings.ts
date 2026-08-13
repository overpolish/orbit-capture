// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  hasOutputComposition,
  RecordingOutputSettings,
} from "./screenshot-output";
import {
  CursorEffectSettings,
  AudioTrackVolume,
  CameraOverlaySettings,
  ExportArtifact,
} from "./types";

export const DEFAULT_COMPRESSION = 2;
export const DEFAULT_CURSOR_EFFECTS: CursorEffectSettings = {
  bake: true,
  clickAnimation: true,
  clipAtVideoEdge: false,
  motionBlur: true,
  sizePercent: 100,
  smoothMovement: true,
};

/**
 * A camera overlay whose crop frame exactly matches the camera image for the
 * given geometry. Using the real output and camera dimensions matters: static
 * percentages put the frame outside the camera for other aspect ratios, which
 * the compositor then has to clamp away from the on-screen controls.
 */
export const cameraOverlayForDimensions = ({
  cameraHeight,
  cameraWidth,
  screenHeight,
  screenWidth,
}: {
  cameraHeight: number;
  cameraWidth: number;
  screenHeight: number;
  screenWidth: number;
}): CameraOverlaySettings => {
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

export const defaultCameraOverlay = (
  artifact?: ExportArtifact | null,
): CameraOverlaySettings => {
  const recording = artifact?.kind === "recording" ? artifact : null;
  const camera = recording?.camera;
  return cameraOverlayForDimensions({
    cameraHeight: camera?.height || 9,
    cameraWidth: camera?.width || 16,
    screenHeight: recording?.height || 9,
    screenWidth: recording?.width || 16,
  });
};

export type VideoExportSettings = {
  compression: number;
  resolutionScalePercent: number;
};

type RecordingSavePlanOptions = {
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  cameraCompression: number;
  cameraOverlay: CameraOverlaySettings;
  cameraResolutionScalePercent: number;
  collapseAudio: boolean;
  compression: number;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  includeCamera: boolean;
  includePrimaryVideo: boolean;
  recordingOutput: RecordingOutputSettings;
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
  audioTrackVolumes,
  bakeCamera,
  camera,
  cameraOverlay,
  collapseAudio,
  compression,
  cursorEffects,
  enabledStreamIndices,
  includeCamera,
  includePrimaryVideo,
  originalResolutionScale,
  recordingOutput,
  resolutionScalePercent,
}: {
  artifact: ExportArtifact | null;
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  camera: VideoExportSettings;
  cameraOverlay: CameraOverlaySettings;
  collapseAudio: boolean;
  compression: number;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[] | null;
  includeCamera: boolean;
  includePrimaryVideo: boolean;
  originalResolutionScale: number;
  recordingOutput: RecordingOutputSettings;
  resolutionScalePercent: number;
}): RecordingSavePlan => {
  const selectedIndices = enabledStreamIndices ?? [];
  const hasCamera =
    artifact?.kind === "recording" && artifact.camera !== null && includeCamera;
  const hasAudioChanges =
    artifact?.kind === "recording" &&
    (selectedIndices.length !== artifact.audioTracks.length ||
      (collapseAudio && selectedIndices.length > 1) ||
      audioTrackVolumes.some((volume) => volume.decibels !== 0));
  const hasPrimaryComposition =
    artifact?.kind === "recording" &&
    hasOutputComposition(recordingOutput.primary, artifact);
  const hasCameraComposition =
    artifact?.kind === "recording" &&
    artifact.camera !== null &&
    hasOutputComposition(recordingOutput.camera, artifact.camera);
  const hasMeasuredWork =
    artifact?.kind === "recording" &&
    artifact.durationMs > 0 &&
    (!includePrimaryVideo ||
      artifact.primaryKind === "audio" ||
      compression > 0 ||
      (artifact.hasCursorData && cursorEffects.bake) ||
      hasPrimaryComposition ||
      resolutionScalePercent < originalResolutionScale ||
      (hasCamera &&
        (bakeCamera ||
          camera.compression > 0 ||
          camera.resolutionScalePercent < 100)) ||
      (hasCamera && hasCameraComposition) ||
      hasAudioChanges);

  return {
    options: {
      audioTrackVolumes,
      bakeCamera: hasCamera && includePrimaryVideo && bakeCamera,
      cameraCompression: camera.compression,
      cameraOverlay,
      cameraResolutionScalePercent: camera.resolutionScalePercent,
      collapseAudio: collapseAudio && selectedIndices.length > 1,
      compression,
      cursorEffects,
      enabledStreamIndices: selectedIndices,
      includeCamera: hasCamera,
      includePrimaryVideo,
      recordingOutput,
      resolutionScalePercent,
    },
    showsMeasuredProgress: hasMeasuredWork || hasCamera,
  };
};
