// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import {
  CameraOverlaySettings,
  ExportSnapshot,
  RecordingPreview,
  RecordingPreviewLayout,
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

export const getRecordingPreview = (artifactId: number) =>
  invoke<RecordingPreview>("get_recording_preview", { artifactId });

export const startRecordingPreviewPlayer = ({
  artifactId,
  enabledStreamIndices,
  eventChannel,
  frameChannel,
  sessionId,
}: {
  artifactId: number;
  enabledStreamIndices: number[];
  eventChannel: Channel<RecordingPreviewPlayerEvent>;
  frameChannel: Channel<ArrayBuffer>;
  sessionId: number;
}) =>
  invoke<RecordingPreviewPlayerInfo>("start_recording_preview_player", {
    artifactId,
    enabledStreamIndices,
    eventChannel,
    frameChannel,
    sessionId,
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

export const stopRecordingPreviewPlayer = (sessionId: number) =>
  invoke<null>("stop_recording_preview_player", { sessionId });

type RecordingProcessingOptions = {
  bakeCamera: boolean;
  cameraCompression: number;
  cameraOverlay: CameraOverlaySettings;
  cameraResolutionScalePercent: number;
  collapseAudio: boolean;
  compression: number;
  enabledStreamIndices: number[];
  resolutionScalePercent: number;
  screenshotRadiusPercent: number;
};

export const estimateRecordingExport = ({
  artifactId,
  bakeCamera,
  cameraCompression,
  cameraOverlay,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  enabledStreamIndices,
  resolutionScalePercent,
  screenshotRadiusPercent,
}: RecordingProcessingOptions & { artifactId: number }) =>
  invoke<number>("estimate_recording_export", {
    artifactId,
    options: {
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      enabledStreamIndices,
      resolutionScalePercent,
      screenshotRadiusPercent,
    },
  });

type SaveExportOptions = RecordingProcessingOptions & {
  fileStem: string;
};

export const saveExport = ({
  bakeCamera,
  cameraCompression,
  cameraOverlay,
  cameraResolutionScalePercent,
  collapseAudio,
  compression,
  enabledStreamIndices,
  fileStem,
  resolutionScalePercent,
  screenshotRadiusPercent,
}: SaveExportOptions) =>
  invoke<string | null>("save_export", {
    fileStem,
    options: {
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      enabledStreamIndices,
      resolutionScalePercent,
      screenshotRadiusPercent,
    },
  });

export const copyExportToClipboard = async (
  screenshotRadiusPercent: number,
) => {
  await invoke<null>("copy_export_to_clipboard", { screenshotRadiusPercent });
};

export const setScreenshotRadius = async (radiusPercent: number) => {
  await invoke<null>("set_screenshot_radius", { radiusPercent });
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
