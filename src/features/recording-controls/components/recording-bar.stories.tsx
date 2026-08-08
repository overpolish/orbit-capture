import { type Meta, type StoryObj } from "@storybook/react-vite";

import { RecordingBar } from "./recording-bar";
const meta = {
  args: { hasSelectedMonitor: true },
  component: RecordingBar,
  parameters: {
    layout: "centered",
  },
  title: "Start Recording/Recording Bar",
} satisfies Meta<typeof RecordingBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const NoMonitorSelected: Story = {
  args: { hasSelectedMonitor: false },
};

export const PermissionsLocked: Story = {
  args: { isLocked: true },
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

export const CameraOnlyEnabled: Story = {
  args: { initialInputs: { camera: true }, initialMode: "camera" },
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

export const Starting: Story = {
  args: { status: "starting" },
};
