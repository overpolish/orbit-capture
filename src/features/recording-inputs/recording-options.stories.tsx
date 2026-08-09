// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import { RecordingOptions, RecordingOptionsProps } from "./recording-options";
import { ALL_SYSTEM_AUDIO, DEFAULT_CAMERA, DEFAULT_MICROPHONE } from "./store";
import { InputDevice, SystemAudioSource } from "./types";

const cameras: InputDevice[] = [
  DEFAULT_CAMERA,
  { id: "continuity", label: "Dom’s iPhone Camera" },
  { id: "studio", label: "Studio Display Camera" },
];

const microphones: InputDevice[] = [
  DEFAULT_MICROPHONE,
  { id: "macbook", label: "MacBook Pro Microphone" },
  { id: "studio", label: "Studio Display Microphone" },
];

const audioSources: SystemAudioSource[] = [
  ALL_SYSTEM_AUDIO,
  { id: "safari", kind: "application", label: "Safari" },
  { id: "zoom", kind: "application", label: "Zoom" },
];

function StatefulOptions(props: RecordingOptionsProps) {
  const [camera, setCamera] = useState(props.selectedCamera);
  const [cameraFlipped, setCameraFlipped] = useState(props.cameraFlipped);
  const [microphone, setMicrophone] = useState(props.selectedMicrophone);
  const [systemAudio, setSystemAudio] = useState(props.selectedSystemAudio);

  return (
    <RecordingOptions
      {...props}
      cameraFlipped={cameraFlipped}
      onCameraChange={setCamera}
      onCameraFlippedChange={setCameraFlipped}
      onMicrophoneChange={setMicrophone}
      onSystemAudioChange={setSystemAudio}
      selectedCamera={camera}
      selectedMicrophone={microphone}
      selectedSystemAudio={systemAudio}
    />
  );
}

const meta = {
  args: {
    audioSources,
    cameraLocked: false,
    cameras,
    microphoneDecibels: -18,
    microphoneLocked: false,
    microphonePeak: -8,
    microphonePreviewEnabled: true,
    microphones,
    onCameraChange: () => undefined,
    onMicrophoneChange: () => undefined,
    onSystemAudioChange: () => undefined,
    selectedCamera: DEFAULT_CAMERA,
    selectedMicrophone: DEFAULT_MICROPHONE,
    selectedSystemAudio: [ALL_SYSTEM_AUDIO],
    standalone: false,
    systemAudioDecibels: -24,
    systemAudioPeak: -12,
    systemAudioPreviewEnabled: true,
  },
  component: RecordingOptions,
  parameters: {
    layout: "centered",
  },
  render: (args) => <StatefulOptions {...args} />,
  title: "Recording/Options",
} satisfies Meta<typeof RecordingOptions>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const CameraFlipped: Story = {
  args: { cameraFlipped: true },
};

export const InputsDisabled: Story = {
  args: {
    microphonePreviewEnabled: false,
    systemAudioPreviewEnabled: false,
  },
};

export const PermissionsRequired: Story = {
  args: {
    cameraLocked: true,
    microphoneLocked: true,
  },
};
