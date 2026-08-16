// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { type Meta, type StoryObj } from "@storybook/react-vite";

import { RecordingBar } from "./recording-bar";
const meta = {
  args: { hasSelectedMonitor: true },
  component: RecordingBar,
  parameters: {
    layout: "centered",
  },
  title: "Features/Recording Bar",
} satisfies Meta<typeof RecordingBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const NoMonitorSelected: Story = {
  args: { hasSelectedMonitor: false },
};

/** Screen recording denied: nothing on the bar works, so it blurs. */
export const PermissionsLocked: Story = {
  args: { isLocked: true, isScreenshotLocked: true },
};

/** Accessibility denied but screen recording granted: stills still work. */
export const ScreenshotOnly: Story = {
  args: { isLocked: true },
};

/** Mid-save: the button is inert and pulsing until the file actually exists. */
export const ScreenshotPending: Story = {
  args: { screenshotState: "pending" },
};

export const ScreenshotDone: Story = {
  args: { screenshotState: "done" },
};

export const ScreenshotFailed: Story = {
  args: { screenshotState: "failed" },
};

/** An open screenshot workspace accepts another screenshot but not a recording. */
export const ScreenshotWorkspaceOpen: Story = {
  args: { pendingExportKind: "screenshot" },
};

export const ClipboardScreenshotPending: Story = {
  args: { screenshotAction: "clipboard", screenshotState: "pending" },
};

export const ClipboardScreenshotDone: Story = {
  args: { screenshotAction: "clipboard", screenshotState: "done" },
};

export const ClipboardScreenshotFailed: Story = {
  args: { screenshotAction: "clipboard", screenshotState: "failed" },
};

export const OptionalPermissionsLocked: Story = {
  args: { isCameraLocked: true, isMicrophoneLocked: true },
};

export const InputsEnabled: Story = {
  args: {
    initialInputs: {
      camera: true,
      microphone: true,
      showCursor: true,
      systemAudio: true,
    },
  },
};

export const MissingEnabledInputs: Story = {
  args: {
    hasCameraWarning: true,
    hasMicrophoneWarning: true,
    hasSystemAudioWarning: true,
    initialInputs: {
      camera: true,
      microphone: true,
      showCursor: true,
      systemAudio: true,
    },
  },
};

/** Missing sources stay quiet until their corresponding input is enabled. */
export const MissingDisabledInputs: Story = {
  args: {
    hasCameraWarning: true,
    hasMicrophoneWarning: true,
    hasSystemAudioWarning: true,
  },
};

export const Region: Story = {
  args: { initialMode: "region" },
};

export const Window: Story = {
  args: { initialMode: "window" },
};

export const WindowSelected: Story = {
  args: { hasSelectedWindow: true, initialMode: "window" },
};

export const CameraOnly: Story = {
  args: { initialMode: "camera" },
};

export const CameraOnlyPreservesScreenCameraOff: Story = {
  args: { initialInputs: { camera: false }, initialMode: "camera" },
};

export const CameraOnlyMissing: Story = {
  args: {
    hasCameraWarning: true,
    initialMode: "camera",
  },
};

export const CameraOnlyPermissionLocked: Story = {
  args: { initialMode: "camera", isCameraLocked: true },
};

export const AudioOnlyDisabled: Story = {
  args: { initialMode: "audio" },
};

export const AudioOnlyWithMicrophone: Story = {
  args: {
    initialInputs: { microphone: true },
    initialMode: "audio",
  },
};

export const AudioOnlyWithSystemAudio: Story = {
  args: {
    initialInputs: { systemAudio: true },
    initialMode: "audio",
  },
};

export const AudioOnlyWithAllSourcesMissing: Story = {
  args: {
    hasMicrophoneWarning: true,
    hasSystemAudioWarning: true,
    initialInputs: { microphone: true, systemAudio: true },
    initialMode: "audio",
  },
};

export const AudioOnlyWithOneValidSource: Story = {
  args: {
    hasMicrophoneWarning: true,
    initialInputs: { microphone: true, systemAudio: true },
    initialMode: "audio",
  },
};

export const Starting: Story = {
  args: { status: "starting" },
};

/** Half the frames, half the file. */
export const HalfFrameRate: Story = {
  args: { initialFps: 30 },
};
