// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import {
  normalizedScreenshotOutput,
  RecordingOutputSettings,
  ScreenshotWorkspaceOutputSettings,
  normalizedScreenshotWorkspaceOutput,
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
export const getExportPreview = (full = false, itemId?: number) =>
  invoke<ArrayBuffer>("get_export_preview", { full, itemId });

export const renderScreenshotOutputPreview = ({
  artifactId,
  channel,
  output,
  requestId,
}: {
  artifactId: number;
  channel: Channel<ArrayBuffer>;
  output: ScreenshotWorkspaceOutputSettings;
  requestId: number;
}) =>
  invoke<null>("render_screenshot_output_preview", {
    artifactId,
    channel,
    output: normalizedScreenshotWorkspaceOutput(output),
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
        cameraOnTop: recordingOutput.cameraOnTop,
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
    },
  });

export const playRecordingPreview = (sessionId: number) =>
  invoke<null>("play_recording_preview", { sessionId });

export const pauseRecordingPreview = (sessionId: number) =>
  invoke<null>("pause_recording_preview", { sessionId });

export const setRecordingPreviewZoom = (
  sessionId: number,
  zoomPercent: number,
) =>
  invoke<null>("set_recording_preview_zoom", {
    sessionId,
    zoomPercent,
  });

export const centerRecordingPreviewWorkspace = () =>
  invoke<null>("center_recording_preview_workspace");

export const layoutRecordingPreviewSurface = ({
  backdrop,
  bakeCamera,
  cameraOverlay,
  nativeEditor,
  panes,
  recordingOutput,
  requestId,
  scale,
  selection,
  selectionTargets,
  sessionId,
  viewport,
}: {
  backdrop: [number, number, number, number];
  bakeCamera: boolean;
  cameraOverlay: CameraOverlaySettings;
  nativeEditor: boolean;
  panes: {
    index: number;
    rect: { height: number; width: number; x: number; y: number };
  }[];
  recordingOutput: RecordingOutputSettings;
  requestId: number;
  scale: number;
  sessionId: number;
  viewport: { height: number; width: number; x: number; y: number };
  selection?: {
    paneIndex: number;
    radiusPercent: number;
    rect: { height: number; width: number; x: number; y: number };
    cropMode?: boolean;
    image?: { height: number; width: number; x: number; y: number };
    layerId?: number;
  } | null;
  selectionTargets?:
    | {
        paneIndex: number;
        radiusPercent: number;
        rect: { height: number; width: number; x: number; y: number };
        cropMode?: boolean;
        image?: { height: number; width: number; x: number; y: number };
        layerId?: number;
      }[]
    | null;
}) =>
  invoke<null>("layout_recording_preview_surface", {
    layout: {
      backdrop,
      bakeCamera,
      cameraOverlay: normalizedCameraOverlay(cameraOverlay),
      nativeEditor,
      panes,
      recordingOutput: {
        camera: normalizedScreenshotOutput(recordingOutput.camera),
        cameraOnTop: recordingOutput.cameraOnTop,
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
      requestId,
      scale,
      selection: selection ?? null,
      selectionTargets: selectionTargets ?? null,
      sessionId,
      viewport,
    },
  });

export const seekRecordingPreview = ({
  positionMs,
  requestId,
  rough = false,
  selectionVisible,
  sessionId,
}: {
  positionMs: number;
  requestId: number;
  sessionId: number;
  rough?: boolean;
  selectionVisible?: boolean;
}) =>
  invoke<null>("seek_recording_preview", {
    positionMs: Number.isFinite(positionMs)
      ? Math.max(0, Math.round(positionMs))
      : 0,
    requestId,
    rough,
    selectionVisible,
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
      cameraOnTop: recordingOutput.cameraOnTop,
      primary: normalizedScreenshotOutput(recordingOutput.primary),
    },
    sessionId,
  });

export const copyRecordingPreviewFrameToClipboard = ({
  artifactId,
  bakeCamera,
  cameraOverlay,
  cursorEffects,
  positionMs,
  recordingOutput,
}: {
  artifactId: number;
  bakeCamera: boolean;
  cameraOverlay: CameraOverlaySettings;
  cursorEffects: CursorEffectSettings;
  positionMs: number;
  recordingOutput: RecordingOutputSettings;
}) =>
  invoke<null>("copy_recording_preview_frame_to_clipboard", {
    artifactId,
    bakeCamera,
    cameraOverlay: normalizedCameraOverlay(cameraOverlay),
    cursorEffects: normalizedCursorEffects(cursorEffects),
    positionMs: Math.max(0, Math.round(positionMs)),
    recordingOutput: {
      camera: normalizedScreenshotOutput(recordingOutput.camera),
      cameraOnTop: recordingOutput.cameraOnTop,
      primary: normalizedScreenshotOutput(recordingOutput.primary),
    },
  });

/**
 * What the backend's preview platform can actually do. Probed instead of
 * sniffed, so a platform whose native backend lands one piece at a time flips
 * one flag at a time.
 */
export type PreviewCapabilities = {
  nativeRecordingPreview: boolean;
  nativeScreenshotPreview: boolean;
  nativeWorkspaceEditor: boolean;
};

export const previewCapabilities = () =>
  invoke<PreviewCapabilities>("preview_capabilities");

export const startScreenshotPreview = (artifactId: number, sessionId: number) =>
  invoke<null>("start_screenshot_preview", { artifactId, sessionId });

export const layoutScreenshotPreviewSurface = ({
  backdrop,
  interactionOutput,
  nativeEditor,
  output,
  panes,
  scale,
  selection,
  selectionTargets,
  sessionId,
  viewport,
}: {
  backdrop: [number, number, number, number];
  interactionOutput: ScreenshotWorkspaceOutputSettings;
  nativeEditor: boolean;
  output: ScreenshotWorkspaceOutputSettings;
  panes: {
    index: number;
    rect: { height: number; width: number; x: number; y: number };
  }[];
  scale: number;
  sessionId: number;
  viewport: { height: number; width: number; x: number; y: number };
  selection?: {
    paneIndex: number;
    radiusPercent: number;
    rect: { height: number; width: number; x: number; y: number };
    cropMode?: boolean;
    image?: { height: number; width: number; x: number; y: number };
    layerId?: number;
  } | null;
  selectionTargets?:
    | {
        paneIndex: number;
        radiusPercent: number;
        rect: { height: number; width: number; x: number; y: number };
        cropMode?: boolean;
        image?: { height: number; width: number; x: number; y: number };
        layerId?: number;
      }[]
    | null;
}) =>
  invoke<null>("layout_screenshot_preview_surface", {
    backdrop,
    interactionOutput: normalizedScreenshotWorkspaceOutput(interactionOutput),
    nativeEditor,
    output: normalizedScreenshotWorkspaceOutput(output),
    panes,
    scale,
    selection: selection ?? null,
    selectionTargets: selectionTargets ?? null,
    sessionId,
    viewport,
  });

export const refreshScreenshotPreviewSources = (
  artifactId: number,
  sessionId: number,
) =>
  invoke<null>("refresh_screenshot_preview_sources", {
    artifactId,
    sessionId,
  });

export const setScreenshotPreviewZoom = (
  sessionId: number,
  zoomPercent: number,
) =>
  invoke<null>("set_screenshot_preview_zoom", {
    sessionId,
    zoomPercent,
  });

export const centerScreenshotPreviewWorkspace = () =>
  invoke<null>("center_screenshot_preview_workspace");

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
  screenshotOutput: ScreenshotWorkspaceOutputSettings;
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
        cameraOnTop: recordingOutput.cameraOnTop,
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
      resolutionScalePercent,
      screenshotOutput: normalizedScreenshotWorkspaceOutput(screenshotOutput),
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
        cameraOnTop: recordingOutput.cameraOnTop,
        primary: normalizedScreenshotOutput(recordingOutput.primary),
      },
      resolutionScalePercent,
      screenshotOutput: normalizedScreenshotWorkspaceOutput(screenshotOutput),
    },
  });

export const copyExportToClipboard = async (
  screenshotOutput: ScreenshotWorkspaceOutputSettings,
) => {
  await invoke<null>("copy_export_to_clipboard", {
    screenshotOutput: normalizedScreenshotWorkspaceOutput(screenshotOutput),
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
