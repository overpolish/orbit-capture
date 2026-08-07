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
  const [microphone, setMicrophone] = useState(props.selectedMicrophone);
  const [systemAudio, setSystemAudio] = useState(props.selectedSystemAudio);

  return (
    <RecordingOptions
      {...props}
      onCameraChange={setCamera}
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
    cameraEnabled: true,
    cameraLocked: false,
    cameras,
    microphoneDecibels: -18,
    microphoneEnabled: true,
    microphoneLocked: false,
    microphonePeak: -8,
    microphones,
    onCameraChange: () => undefined,
    onMicrophoneChange: () => undefined,
    onSystemAudioChange: () => undefined,
    selectedCamera: DEFAULT_CAMERA,
    selectedMicrophone: DEFAULT_MICROPHONE,
    selectedSystemAudio: [ALL_SYSTEM_AUDIO],
    standalone: false,
    systemAudioDecibels: -24,
    systemAudioEnabled: true,
    systemAudioPeak: -12,
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

export const InputsDisabled: Story = {
  args: {
    cameraEnabled: false,
    microphoneEnabled: false,
    systemAudioEnabled: false,
  },
};

export const PermissionsRequired: Story = {
  args: {
    cameraLocked: true,
    microphoneLocked: true,
  },
};
