// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

type ExportArtifactBase = {
  extension: string;
  height: number;
  /** Unique per capture, so a replacement is never mistaken for the same one. */
  id: number;
  suggestedFileStem: string;
  width: number;
};

type AudioTrackKind = "microphone" | "system-audio" | "unknown";

type RecordingAudioTrack = {
  kind: AudioTrackKind;
  label: string;
  streamIndex: number;
};

export type AudioTrackVolume = {
  decibels: number;
  streamIndex: number;
};

export type PreparedAudioTrack = {
  kind: AudioTrackKind;
  label: string;
  /**
   * Which recorded track this came from and what identifies its row on screen.
   */
  streamIndex: number;
  waveform: number[];
};

export type RecordingPreview = {
  artifactId: number;
  tracks: PreparedAudioTrack[];
};

export type RecordingPreviewPane = {
  height: number;
  kind: "camera" | "screen";
  sourceHeight: number;
  sourceWidth: number;
  width: number;
  x: number;
  y: number;
};

export type RecordingPreviewLayout = {
  height: number;
  panes: RecordingPreviewPane[];
  width: number;
};

export type RecordingVideoTrackId = "camera" | "primary";
export type RecordingTrackId = RecordingVideoTrackId | `audio:${number}`;
export type RecordingTimelineThumbnail = {
  id: string;
  url: string | null;
};
export type RecordingTimelineThumbnails = Record<
  RecordingVideoTrackId,
  RecordingTimelineThumbnail[]
>;

export type CursorEffectSettings = {
  bake: boolean;
  clickAnimation: boolean;
  motionBlur: boolean;
  sizePercent: number;
  smoothMovement: boolean;
};

export const recordingAudioTrackId = (streamIndex: number): RecordingTrackId =>
  `audio:${String(streamIndex)}` as RecordingTrackId;

export const recordingAudioStreamIndex = (trackId: RecordingTrackId | null) => {
  if (!trackId?.startsWith("audio:")) return null;
  const streamIndex = Number(trackId.slice("audio:".length));
  return Number.isInteger(streamIndex) ? streamIndex : null;
};

type RecordingCamera = {
  durationMs: number;
  height: number;
  originalSizeBytes: number;
  path: string;
  width: number;
};

export type CameraOverlaySettings = {
  /** Camera image centre in screen-recording coordinates. */
  cameraWidthPercent: number;
  cameraXPercent: number;
  cameraYPercent: number;
  /** Crop-window rectangle in screen-recording coordinates. */
  frameHeightPercent: number;
  frameWidthPercent: number;
  frameXPercent: number;
  frameYPercent: number;
  /** Corner radius as a percentage of the camera frame's shorter edge. */
  radiusPercent: number;
};

/**
 * A capture waiting to be exported. The window switches on `kind` rather than
 * assuming a screenshot: a recording is a file that gets moved, not pixels
 * that get encoded, and almost nothing about handling the two is the same.
 */
export type ExportArtifact =
  | (ExportArtifactBase & {
      audioTracks: RecordingAudioTrack[];
      camera: RecordingCamera | null;
      canCompress: boolean;
      cursorDataVersion: number | null;
      /** Zero for a recording recovered from an earlier run, whose length is unknown. */
      durationMs: number;
      hasCursorData: boolean;
      kind: "recording";
      originalSizeBytes: number;
      /** The working recording consumed by the native preview and export paths. */
      path: string;
      primaryKind: "audio" | "camera" | "screen";
      /** Captured pixels per logical display point, multiplied by 100. */
      sourceScalePercent: number;
    })
  | (ExportArtifactBase & { kind: "screenshot" });

export type ExportSnapshot = {
  artifact: ExportArtifact | null;
  cursorEffects: CursorEffectSettings;
  directory: string | null;
  screenshotRadiusPercent: number;
};

export const initialExportSnapshot: ExportSnapshot = {
  artifact: null,
  cursorEffects: {
    bake: true,
    clickAnimation: true,
    motionBlur: true,
    sizePercent: 100,
    smoothMovement: true,
  },
  directory: null,
  screenshotRadiusPercent: 0,
};
