// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useRef, useState } from "react";

import { RecordingOptions, RecordingOptionsProps } from "./recording-options";
import {
  ALL_SYSTEM_AUDIO,
  DEFAULT_CAMERA,
  DEFAULT_CAMERA_MODE,
  DEFAULT_MICROPHONE,
} from "./store";
import { CameraDevice, InputDevice, SystemAudioSource } from "./types";

const cameras: CameraDevice[] = [
  DEFAULT_CAMERA,
  {
    id: "continuity",
    label: "Dom’s iPhone Camera",
    modes: [
      {
        fps: 60,
        height: 1080,
        id: "1920x1080@60",
        isDefault: true,
        label: "1920 × 1080",
        width: 1920,
      },
      {
        fps: 60,
        height: 1920,
        id: "1080x1920@60",
        label: "1080 × 1920",
        width: 1080,
      },
      {
        fps: 60,
        height: 1440,
        id: "1920x1440@60",
        label: "1920 × 1440",
        width: 1920,
      },
    ],
  },
  {
    id: "studio",
    label: "Studio Display Camera",
    modes: [DEFAULT_CAMERA_MODE],
  },
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
  const [cameraResolution, setCameraResolution] = useState(
    props.selectedCameraResolution,
  );
  const [cameraFlipped, setCameraFlipped] = useState(props.cameraFlipped);
  const [microphone, setMicrophone] = useState(props.selectedMicrophone);
  const [systemAudio, setSystemAudio] = useState(props.selectedSystemAudio);
  const cameraPreviewRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = cameraPreviewRef.current;
    if (!canvas || !props.cameraPreviewActive || !cameraResolution) return;
    canvas.width = cameraResolution.width;
    canvas.height = cameraResolution.height;
    const context = canvas.getContext("2d");
    if (!context) return;
    const gradient = context.createLinearGradient(
      0,
      0,
      cameraResolution.width,
      cameraResolution.height,
    );
    gradient.addColorStop(0, "#2563eb");
    gradient.addColorStop(1, "#16a34a");
    context.fillStyle = gradient;
    context.fillRect(0, 0, cameraResolution.width, cameraResolution.height);
  }, [cameraResolution, props.cameraPreviewActive]);

  return (
    <RecordingOptions
      {...props}
      cameraFlipped={cameraFlipped}
      cameraPreviewRef={cameraPreviewRef}
      onCameraChange={setCamera}
      onCameraFlippedChange={setCameraFlipped}
      onCameraResolutionChange={setCameraResolution}
      onMicrophoneChange={setMicrophone}
      onSystemAudioChange={setSystemAudio}
      selectedCamera={camera}
      selectedCameraResolution={cameraResolution}
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
    onCameraResolutionChange: () => undefined,
    onMicrophoneChange: () => undefined,
    onSystemAudioChange: () => undefined,
    selectedCamera: DEFAULT_CAMERA,
    selectedCameraResolution: DEFAULT_CAMERA_MODE,
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
  title: "Features/Recording Options",
} satisfies Meta<typeof RecordingOptions>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const CameraFlipped: Story = {
  args: { cameraFlipped: true },
};

export const PortraitCameraPreview: Story = {
  args: {
    cameraPreviewActive: true,
    selectedCamera: cameras[1],
    selectedCameraResolution: cameras[1].modes[1],
  },
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
