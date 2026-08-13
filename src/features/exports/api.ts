// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import {
  normalizedScreenshotOutput,
  RecordingOutputSettings,
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

const finite = (value: number, fallback: number) =>
  Number.isFinite(value) ? value : fallback;

const normalizedCameraOverlay = (
  settings: CameraOverlaySettings,
): CameraOverlaySettings => ({
  cameraWidthPercent: finite(settings.cameraWidthPercent, 25),
  cameraXPercent: finite(settings.cameraXPercent, 85),
  cameraYPercent: finite(settings.cameraYPercent, 15),
  frameHeightPercent: finite(settings.frameHeightPercent, 25),
  frameWidthPercent: finite(settings.frameWidthPercent, 25),
  frameXPercent: finite(settings.frameXPercent, 72),
  frameYPercent: finite(settings.frameYPercent, 3),
  radiusPercent: finite(settings.radiusPercent, 8),
});

const normalizedCursorEffects = (
  settings: CursorEffectSettings,
): CursorEffectSettings => ({
  ...settings,
  sizePercent: finite(settings.sizePercent, 100),
});

const normalizedAudioTrackVolumes = (volumes: AudioTrackVolume[]) =>
  volumes.map((volume) => ({
    ...volume,
    decibels: finite(volume.decibels, 0),
  }));

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

export const renderScreenshotOutputPreview = ({
  artifactId,
  channel,
  output,
  requestId,
}: {
  artifactId: number;
  channel: Channel<ArrayBuffer>;
  output: ScreenshotOutputSettings;
  requestId: number;
}) =>
  invoke<null>("render_screenshot_output_preview", {
    artifactId,
    channel,
    output: normalizedScreenshotOutput(output),
    requestId,
  });

export const getRecordingPreview = (artifactId: number) =>
  invoke<RecordingPreview>("get_recording_preview", { artifactId });

export const startRecordingPreviewPlayer = ({
  artifactId,
  audioTrackVolumes,
  bakeCamera,
  cameraOverlay,
  cursorEffects,
  enabledStreamIndices,
  eventChannel,
  frameChannel,
  recordingOutput,
  sessionId,
}: {
  artifactId: number;
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  cameraOverlay: CameraOverlaySettings;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  eventChannel: Channel<RecordingPreviewPlayerEvent>;
  frameChannel: Channel<ArrayBuffer>;
  recordingOutput: RecordingOutputSettings;
  sessionId: number;
}) =>
  invoke<RecordingPreviewPlayerInfo>("start_recording_preview_player", {
    artifactId,
    eventChannel,
    frameChannel,
    sessionId,
    settings: {
      audio: {
        audioTrackVolumes: normalizedAudioTrackVolumes(audioTrackVolumes),
        enabledStreamIndices,
      },
      bakeCamera,
      cameraOverlay: normalizedCameraOverlay(cameraOverlay),
      cursorEffects: normalizedCursorEffects(cursorEffects),
      recordingOutput: {
        camera: normalizedScreenshotOutput(recordingOutput.camera),
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
    },
  });

export const playRecordingPreview = (sessionId: number) =>
  invoke<null>("play_recording_preview", { sessionId });

export const pauseRecordingPreview = (sessionId: number) =>
  invoke<null>("pause_recording_preview", { sessionId });

export const layoutRecordingPreviewSurface = ({
  backdrop,
  panes,
  requestId,
  scale,
  sessionId,
  viewport,
}: {
  backdrop: [number, number, number];
  panes: {
    index: number;
    rect: { height: number; width: number; x: number; y: number };
  }[];
  requestId: number;
  scale: number;
  sessionId: number;
  viewport: { height: number; width: number; x: number; y: number };
}) =>
  invoke<null>("layout_recording_preview_surface", {
    backdrop,
    panes,
    requestId,
    scale,
    sessionId,
    viewport,
  });

export const requestRecordingPreviewFullResolution = (sessionId: number) =>
  invoke<null>("request_recording_preview_full_resolution", { sessionId });

export const seekRecordingPreview = ({
  positionMs,
  requestId,
  rough = false,
  sessionId,
}: {
  positionMs: number;
  requestId: number;
  sessionId: number;
  rough?: boolean;
}) =>
  invoke<null>("seek_recording_preview", {
    positionMs: Number.isFinite(positionMs)
      ? Math.max(0, Math.round(positionMs))
      : 0,
    requestId,
    rough,
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
    audioTrackVolumes: normalizedAudioTrackVolumes(audioTrackVolumes),
    sessionId,
  });

export const setRecordingPreviewCursorEffects = (
  cursorEffects: CursorEffectSettings,
  sessionId: number,
) =>
  invoke<null>("set_recording_preview_cursor_effects", {
    cursorEffects: normalizedCursorEffects(cursorEffects),
    sessionId,
  });

export const setRecordingPreviewComposition = ({
  bakeCamera,
  cameraOverlay,
  recordingOutput,
  sessionId,
}: {
  bakeCamera: boolean;
  cameraOverlay: CameraOverlaySettings;
  recordingOutput: RecordingOutputSettings;
  sessionId: number;
}) =>
  invoke<null>("set_recording_preview_composition", {
    bakeCamera,
    cameraOverlay: normalizedCameraOverlay(cameraOverlay),
    recordingOutput: {
      camera: normalizedScreenshotOutput(recordingOutput.camera),
      primary: normalizedScreenshotOutput(recordingOutput.primary),
    },
    sessionId,
  });

export const copyRecordingPreviewSourceFrame = ({
  artifactId,
  positionMs,
  track,
}: {
  artifactId: number;
  positionMs: number;
  track: 0 | 1;
}) =>
  invoke<ArrayBuffer>("copy_recording_preview_source_frame", {
    artifactId,
    positionMs: Math.max(0, Math.round(positionMs)),
    track,
  });

/**
 * What the backend's preview platform can actually do. Probed instead of
 * sniffed, so a platform whose native backend lands one piece at a time flips
 * one flag at a time.
 */
export type PreviewCapabilities = {
  nativeRecordingPreview: boolean;
  nativeScreenshotPreview: boolean;
};

export const previewCapabilities = () =>
  invoke<PreviewCapabilities>("preview_capabilities");

export const startScreenshotPreview = (artifactId: number, sessionId: number) =>
  invoke<null>("start_screenshot_preview", { artifactId, sessionId });

export const layoutScreenshotPreviewSurface = ({
  backdrop,
  panes,
  scale,
  sessionId,
  viewport,
}: {
  backdrop: [number, number, number];
  panes: {
    index: number;
    rect: { height: number; width: number; x: number; y: number };
  }[];
  scale: number;
  sessionId: number;
  viewport: { height: number; width: number; x: number; y: number };
}) =>
  invoke<null>("layout_screenshot_preview_surface", {
    backdrop,
    panes,
    scale,
    sessionId,
    viewport,
  });

export const setScreenshotPreviewOutput = (
  output: ScreenshotOutputSettings,
  sessionId: number,
) =>
  invoke<null>("set_screenshot_preview_output", {
    output: normalizedScreenshotOutput(output),
    sessionId,
  });

export const stopScreenshotPreview = (sessionId: number) =>
  invoke<null>("stop_screenshot_preview", { sessionId });

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
  recordingOutput: RecordingOutputSettings;
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
  recordingOutput,
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
      recordingOutput: {
        camera: normalizedScreenshotOutput(recordingOutput.camera),
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
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
  recordingOutput,
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
      recordingOutput: {
        camera: normalizedScreenshotOutput(recordingOutput.camera),
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
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
