// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import { MeshGradientPoint } from "./screenshot-background";
import {
  normalizedScreenshotOutput,
  ScreenshotOutputSettings,
} from "./screenshot-output";
import {
  CameraOverlaySettings,
  AudioTrackVolume,
  ExportSnapshot,
  RecordingPreview,
  RecordingPreviewLayout,
  CursorEffectSettings,
} from "./types";

export type RecordingPreviewPlayerEvent =
  | { event: "ended" }
  | { data: { message: string }; event: "error" }
  | {
      data: { positionMs: number };
      event: "paused" | "playing" | "position";
    }
  | { data: { positionMs: number; requestId: number }; event: "ready" };

export type RecordingPreviewPlayerInfo = {
  durationMs: number;
  layout: RecordingPreviewLayout;
};

export const getExportSnapshot = () =>
  invoke<ExportSnapshot>("get_export_snapshot");

/**
 * Raw PNG bytes. The thumbnail by default; the full capture only when
 * something zooms in far enough to need the real pixels.
 */
export const getExportPreview = (full = false) =>
  invoke<ArrayBuffer>("get_export_preview", { full });

export const renderMeshBackgroundPreview = ({
  channel,
  colors,
  height,
  points,
  requestId,
  seed,
  warpPercent,
  width,
}: {
  channel: Channel<ArrayBuffer>;
  colors: string[];
  height: number;
  points: MeshGradientPoint[];
  requestId: number;
  seed: number;
  warpPercent: number;
  width: number;
}) =>
  invoke<null>("render_mesh_background_preview", {
    channel,
    options: { colors, height, points, requestId, seed, warpPercent, width },
  });

export const getRecordingPreview = (artifactId: number) =>
  invoke<RecordingPreview>("get_recording_preview", { artifactId });

export const startRecordingPreviewPlayer = ({
  artifactId,
  audioTrackVolumes,
  cursorEffects,
  enabledStreamIndices,
  eventChannel,
  frameChannel,
  sessionId,
}: {
  artifactId: number;
  audioTrackVolumes: AudioTrackVolume[];
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  eventChannel: Channel<RecordingPreviewPlayerEvent>;
  frameChannel: Channel<ArrayBuffer>;
  sessionId: number;
}) =>
  invoke<RecordingPreviewPlayerInfo>("start_recording_preview_player", {
    artifactId,
    eventChannel,
    frameChannel,
    sessionId,
    settings: {
      audio: { audioTrackVolumes, enabledStreamIndices },
      cursorEffects,
    },
  });

export const playRecordingPreview = (sessionId: number) =>
  invoke<null>("play_recording_preview", { sessionId });

export const pauseRecordingPreview = (sessionId: number) =>
  invoke<null>("pause_recording_preview", { sessionId });

export const requestRecordingPreviewFullResolution = (sessionId: number) =>
  invoke<null>("request_recording_preview_full_resolution", { sessionId });

export const seekRecordingPreview = (
  positionMs: number,
  requestId: number,
  sessionId: number,
) =>
  invoke<null>("seek_recording_preview", {
    positionMs: Number.isFinite(positionMs)
      ? Math.max(0, Math.round(positionMs))
      : 0,
    requestId,
    sessionId,
  });

export const selectRecordingPreviewAudio = (
  enabledStreamIndices: number[],
  sessionId: number,
) =>
  invoke<null>("select_recording_preview_audio", {
    enabledStreamIndices,
    sessionId,
  });

export const setRecordingPreviewAudioVolumes = (
  audioTrackVolumes: AudioTrackVolume[],
  sessionId: number,
) =>
  invoke<null>("set_recording_preview_audio_volumes", {
    audioTrackVolumes,
    sessionId,
  });

export const setRecordingPreviewCursorEffects = (
  cursorEffects: CursorEffectSettings,
  sessionId: number,
) =>
  invoke<null>("set_recording_preview_cursor_effects", {
    cursorEffects,
    sessionId,
  });

export const stopRecordingPreviewPlayer = (sessionId: number) =>
  invoke<null>("stop_recording_preview_player", { sessionId });

export const streamRecordingTimelineThumbnails = (
  artifactId: number,
  count: number,
  channel: Channel<ArrayBuffer>,
) =>
  invoke<null>("stream_recording_timeline_thumbnails", {
    artifactId,
    channel,
    count,
  });

type RecordingProcessingOptions = {
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
  resolutionScalePercent: number;
  screenshotOutput: ScreenshotOutputSettings;
};

export const estimateRecordingExport = ({
  artifactId,
  audioTrackVolumes,
  bakeCamera,
  cameraCompression,
  cameraOverlay,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  cursorEffects,
  enabledStreamIndices,
  includeCamera,
  includePrimaryVideo,
  resolutionScalePercent,
  screenshotOutput,
}: RecordingProcessingOptions & { artifactId: number }) =>
  invoke<number>("estimate_recording_export", {
    artifactId,
    options: {
      audioTrackVolumes,
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      cursorEffects,
      enabledStreamIndices,
      includeCamera,
      includePrimaryVideo,
      resolutionScalePercent,
      screenshotOutput: normalizedScreenshotOutput(screenshotOutput),
    },
  });

type SaveExportOptions = RecordingProcessingOptions & {
  fileStem: string;
};

export const saveExport = ({
  audioTrackVolumes,
  bakeCamera,
  cameraCompression,
  cameraOverlay,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  cursorEffects,
  enabledStreamIndices,
  fileStem,
  includeCamera,
  includePrimaryVideo,
  resolutionScalePercent,
  screenshotOutput,
}: SaveExportOptions) =>
  invoke<string | null>("save_export", {
    fileStem,
    options: {
      audioTrackVolumes,
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      cursorEffects,
      enabledStreamIndices,
      includeCamera,
      includePrimaryVideo,
      resolutionScalePercent,
      screenshotOutput: normalizedScreenshotOutput(screenshotOutput),
    },
  });

export const copyExportToClipboard = async (
  screenshotOutput: ScreenshotOutputSettings,
) => {
  await invoke<null>("copy_export_to_clipboard", {
    screenshotOutput: normalizedScreenshotOutput(screenshotOutput),
  });
};

export const setScreenshotRadius = async (radiusPercent: number) => {
  await invoke<null>("set_screenshot_radius", { radiusPercent });
};

export const setScreenshotBackgroundRadius = async (radiusPercent: number) => {
  await invoke<null>("set_screenshot_background_radius", { radiusPercent });
};

export const cancelExport = async () => {
  await invoke<null>("cancel_export");
};

export const cancelExportJob = () => invoke<boolean>("cancel_export_job");

export const browseExportDirectory = () =>
  invoke<string | null>("browse_export_directory");

export const setExportDirectory = async (directory: string) => {
  await invoke<null>("set_export_directory", { directory });
};
