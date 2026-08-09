import { type Meta, type StoryObj } from "@storybook/react-vite";

import { ExportArtifact } from "../types";

import { ExportPanel } from "./export-panel";

const screenshot: ExportArtifact = {
  extension: "png",
  height: 2234,
  id: 1,
  kind: "screenshot",
  suggestedFileStem: "Orbit Capture 2026-08-08 at 14.32.05",
  width: 3456,
};

const recording: ExportArtifact = {
  audioTracks: [
    { kind: "system-audio", label: "System audio", streamIndex: 0 },
    { kind: "microphone", label: "Microphone", streamIndex: 1 },
  ],
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
  sourceScalePercent: 200,
  suggestedFileStem: "Orbit Capture 2026-08-08 at 14.32.05",
  width: 3840,
};

const meta = {
  args: {
    artifact: screenshot,
    directory: "/Users/dom/Desktop",
    fileStem: screenshot.suggestedFileStem,
  },
  component: ExportPanel,
  parameters: {
    layout: "centered",
  },
  title: "Export/Export Panel",
} satisfies Meta<typeof ExportPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

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

/** A finished recording: a poster, how long it runs, and no clipboard offer. */
export const Recording: Story = {
  args: {
    artifact: recording,
    compression: 2,
    enabledAudioTrackCount: 2,
    estimatedSizeBytes: 74_200_000,
    fileStem: recording.suggestedFileStem,
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
};

export const RecordingWithCollapsedAudio: Story = {
  args: {
    ...Recording.args,
    collapseAudio: true,
  },
};

export const SavingRecording: Story = {
  args: {
    ...Recording.args,
    isSaving: true,
    saveProgress: 58,
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

export const PreparingRecordingAudio: Story = {
  args: {
    artifact: recording,
    fileStem: recording.suggestedFileStem,
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
    fileStem: recording.suggestedFileStem,
  },
};
