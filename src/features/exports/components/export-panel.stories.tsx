// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";
import { ComponentProps, useState } from "react";

import { ExportArtifact } from "../types";

import { ExportPanel } from "./export-panel";
import screenshotPreview from "./export-panel-preview.svg";
import {
  AudioRecordingStoryPanel,
  RecordingStoryPanel,
} from "./export-panel-story-panels";

const screenshot: ExportArtifact = {
  extension: "png",
  height: 2234,
  id: 1,
  kind: "screenshot",
  suggestedFileStem: "Orbit Capture 2026-08-08 at 14.32.05",
  width: 3456,
};

const recording: Extract<ExportArtifact, { kind: "recording" }> = {
  audioTracks: [
    { kind: "system-audio", label: "System audio", streamIndex: 0 },
    { kind: "microphone", label: "Microphone", streamIndex: 1 },
  ],
  camera: null,
  canCompress: true,
  durationMs: 3_845_000,
  extension: "mp4",
  height: 2160,
  id: 2,
  kind: "recording",
  originalSizeBytes: 186_400_000,
  // The working file is a QuickTime movie; `extension` is what saving it
  // delivers, which is not the same thing.
  path: "/tmp/Recordings/recording-20260808-143205.000.mov",
  primaryKind: "screen",
  sourceScalePercent: 200,
  suggestedFileStem: "Orbit Capture 2026-08-08 at 14.32.05",
  width: 3840,
};

const screenPreviewLayout = {
  height: 720,
  panes: [
    {
      height: 720,
      kind: "screen" as const,
      sourceHeight: 2160,
      sourceWidth: 3840,
      width: 1280,
      x: 0,
      y: 0,
    },
  ],
  width: 1280,
};

const cameraPreviewLayout = {
  height: 720,
  panes: [
    ...screenPreviewLayout.panes,
    {
      height: 720,
      kind: "camera" as const,
      sourceHeight: 1080,
      sourceWidth: 1920,
      width: 1280,
      x: 1280,
      y: 0,
    },
  ],
  width: 2560,
};

const cameraOnlyPreviewLayout = {
  height: 720,
  panes: [
    {
      height: 720,
      kind: "camera" as const,
      sourceHeight: 1080,
      sourceWidth: 1920,
      width: 1280,
      x: 0,
      y: 0,
    },
  ],
  width: 1280,
};

const audioPreviewLayout = {
  height: 0,
  panes: [],
  width: 0,
};

const meta = {
  args: {
    artifact: screenshot,
    directory: "/Users/dom/Desktop",
    fileStem: screenshot.suggestedFileStem,
  },
  component: ExportPanel,
  parameters: {
    layout: "fullscreen",
  },
  title: "Export/Export Panel",
} satisfies Meta<typeof ExportPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

function RoundedScreenshotPanel(args: ComponentProps<typeof ExportPanel>) {
  const [radius, setRadius] = useState(args.screenshotRadiusPercent ?? 12);
  return (
    <ExportPanel
      {...args}
      onScreenshotRadiusChange={setRadius}
      screenshotRadiusPercent={radius}
    />
  );
}

export const RoundedScreenshot: Story = {
  args: {
    previewUrl: screenshotPreview,
    screenshotRadiusPercent: 12,
  },
  render: (args) => <RoundedScreenshotPanel {...args} />,
};

export const Saving: Story = {
  args: { isSaving: true },
};

/** A name that has been emptied cannot be saved. */
export const EmptyName: Story = {
  args: { fileStem: "" },
};

export const WithError: Story = {
  args: { error: "That file name cannot be used" },
};

/** A long path truncates rather than pushing the Choose button off the row. */
export const LongDestination: Story = {
  args: {
    directory:
      "/Users/dom/Library/Mobile Documents/com~apple~CloudDocs/Screenshots/2026/August",
  },
};

/** Cancelling clears the artifact, which is what the window then shows. */
export const NothingPending: Story = {
  args: { artifact: null, fileStem: "" },
};

export const Recording: Story = {
  args: {
    artifact: recording,
    compression: 2,
    enabledAudioTrackCount: 2,
    enabledVideoTracks: ["primary"],
    estimatedSizeBytes: 74_200_000,
    fileStem: recording.suggestedFileStem,
    recordingPreviewLayout: screenPreviewLayout,
    recordingPreviewTracks: [
      {
        kind: "system-audio",
        label: "System audio",
        streamIndex: 0,
        waveform: Array.from(
          { length: 96 },
          (_, index) => Math.abs(Math.sin(index * 0.31)) * 0.8,
        ),
      },
      {
        kind: "microphone",
        label: "Microphone",
        streamIndex: 1,
        waveform: Array.from(
          { length: 96 },
          (_, index) => Math.abs(Math.sin(index * 0.17)) * 0.55,
        ),
      },
    ],
    resolutionScalePercent: 150,
  },
  render: (args) => <RecordingStoryPanel {...args} />,
};

export const RecordingWithCollapsedAudio: Story = {
  args: {
    ...Recording.args,
    collapseAudio: true,
  },
  render: (args) => <RecordingStoryPanel {...args} />,
};

/** Export is unavailable once every recorded track has been excluded. */
export const RecordingWithNothingSelected: Story = {
  args: {
    ...Recording.args,
    enabledAudioTrackCount: 0,
    enabledStreamIndices: [],
    enabledVideoTracks: [],
  },
};

/** Camera-only capture is the primary movie, not a screen sidecar to bake. */
export const CameraRecording: Story = {
  args: {
    ...Recording.args,
    artifact: {
      ...recording,
      height: 1080,
      primaryKind: "camera",
      sourceScalePercent: 100,
      width: 1920,
    },
    recordingPreviewLayout: cameraOnlyPreviewLayout,
    resolutionScalePercent: 75,
  },
  render: (args) => <RecordingStoryPanel {...args} />,
};

/** Audio-only capture has transport, waveforms and tracks, but no video controls. */
export const AudioRecording: Story = {
  args: {
    ...Recording.args,
    artifact: {
      ...recording,
      canCompress: false,
      extension: "m4a",
      height: 0,
      path: "/tmp/Recordings/audio-20260808-143205.000.mov",
      primaryKind: "audio",
      sourceScalePercent: 100,
      width: 0,
    },
    enabledVideoTracks: [],
    estimatedSizeBytes: null,
    recordingPreviewLayout: audioPreviewLayout,
    resolutionScalePercent: 100,
  },
  render: (args) => <AudioRecordingStoryPanel {...args} />,
};

/** Screen and camera captures remain separate, synchronized preview panels. */
export const RecordingWithCamera: Story = {
  args: {
    ...Recording.args,
    artifact: {
      ...recording,
      camera: {
        durationMs: recording.durationMs,
        height: 1080,
        originalSizeBytes: 18_400_000,
        path: "/tmp/Recordings/camera-20260808-143205.000.mov",
        width: 1920,
      },
    },
    cameraCompression: 2,
    cameraResolutionScalePercent: 100,
    enabledVideoTracks: ["primary", "camera"],
    recordingPreviewLayout: cameraPreviewLayout,
  },
  render: (args) => <RecordingStoryPanel {...args} />,
};

export const RecordingWithBakedCamera: Story = {
  args: {
    ...RecordingWithCamera.args,
    bakeCamera: true,
    cameraOverlay: {
      cameraWidthPercent: 25,
      cameraXPercent: 85,
      cameraYPercent: 18,
      frameHeightPercent: 14.0625,
      frameWidthPercent: 25,
      frameXPercent: 72,
      frameYPercent: 4,
      radiusPercent: 8,
    },
  },
  render: (args) => <RecordingStoryPanel {...args} />,
};

export const SavingRecording: Story = {
  args: {
    ...Recording.args,
    isSaving: true,
    saveProgress: 58,
  },
};

export const SavingCamera: Story = {
  args: {
    ...RecordingWithCamera.args,
    isSaving: true,
    savePhase: "camera",
    saveProgress: 68,
  },
};

export const CancelingRecording: Story = {
  args: {
    ...SavingRecording.args,
    isCancelingSave: true,
  },
};

export const EstimatingCompressedSize: Story = {
  args: {
    ...Recording.args,
    estimatedSizeBytes: null,
    isEstimatingSize: true,
    isExportPreparationPending: true,
  },
};

export const RecordingWithoutCompressionSupport: Story = {
  args: {
    ...Recording.args,
    artifact: { ...recording, canCompress: false },
    compression: 0,
    estimatedSizeBytes: recording.originalSizeBytes,
  },
};

export const PreparingRecordingPreview: Story = {
  args: {
    ...Recording.args,
    isPreparingRecordingPreview: true,
  },
};

/**
 * A recording recovered from a previous run. Its frames are long gone, so it
 * has neither a poster nor a length - only the file and a name for it.
 */
export const RecoveredRecording: Story = {
  args: {
    artifact: {
      ...recording,
      audioTracks: [],
      durationMs: 0,
      height: 0,
      originalSizeBytes: 0,
      width: 0,
    },
    enabledVideoTracks: ["primary"],
    fileStem: recording.suggestedFileStem,
  },
};
