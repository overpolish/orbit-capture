// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Camera,
  CameraOff,
  FlipHorizontal2,
  Lock,
  Scan,
  Mic,
  Volume2,
} from "lucide-react";
import { RefObject } from "react";
import { TooltipTrigger } from "react-aria-components";

import { Button } from "../../components/base/button/button";
import { ToggleButton } from "../../components/base/button/toggle-button";
import { CircularProgressBar } from "../../components/base/circular-progress-bar/circular-progress-bar";
import { ListBoxItem } from "../../components/base/listbox-item/listbox-item";
import { Select } from "../../components/base/select/select";
import { Tooltip } from "../../components/base/tooltip/tooltip";
import { cn } from "../../lib/styling";
import { AudioMeter } from "../audio-inputs/components/audio-meter";
import { StandaloneMultiSelect } from "../standalone-listbox/standalone-multi-select";
import { StandaloneSelect } from "../standalone-listbox/standalone-select";

import {
  CameraDevice,
  CameraResolution,
  InputDevice,
  SystemAudioSource,
} from "./types";

type InputSelectProps<T extends InputDevice> = {
  icon: React.ReactNode;
  id: string;
  items: T[];
  label: string;
  onChange: (item: T) => void;
  placeholder: string;
  selected: T | null;
  standalone: boolean;
  onOpen?: () => Promise<T[]>;
};

function InputSelect<T extends InputDevice>({
  icon,
  id,
  items,
  label,
  onChange,
  onOpen,
  placeholder,
  selected,
  standalone,
}: InputSelectProps<T>) {
  if (standalone) {
    return (
      <StandaloneSelect
        id={id}
        items={items}
        label={label}
        leftSection={icon}
        onOpen={onOpen}
        onSelectionChange={(item) => {
          const match = items.find((candidate) => candidate.id === item.id);
          if (match) onChange(match);
        }}
        placeholder={placeholder}
        selectedId={selected?.id ?? null}
      />
    );
  }

  return (
    <Select<T>
      aria-label={label}
      className="w-full"
      clearable={false}
      items={items}
      leftSection={icon}
      onChange={(selection) => {
        const match = items.find((item) => item.id === selection);
        if (match) onChange(match);
      }}
      placeholder={placeholder}
      showFocus={false}
      size="sm"
      value={selected?.id ?? null}
      variant="ghost"
    >
      {(item: T) => (
        <ListBoxItem id={item.id} textValue={item.label}>
          {item.label}
        </ListBoxItem>
      )}
    </Select>
  );
}

type SystemAudioSelectProps = {
  items: SystemAudioSource[];
  onChange: (items: SystemAudioSource[]) => void;
  selected: SystemAudioSource[];
  standalone: boolean;
  onOpen?: () => Promise<SystemAudioSource[]>;
};

function SystemAudioSelect({
  items,
  onChange,
  onOpen,
  selected,
  standalone,
}: SystemAudioSelectProps) {
  if (standalone) {
    return (
      <StandaloneMultiSelect
        exclusiveId="all"
        id="system-audio"
        items={items}
        label="System audio"
        leftSection={<Volume2 size={14} />}
        onOpen={onOpen}
        onSelectionChange={(selection) => {
          onChange(
            selection
              .map((item) =>
                items.find((candidate) => candidate.id === item.id),
              )
              .filter((item): item is SystemAudioSource => item !== undefined),
          );
        }}
        placeholder="No system audio"
        selectedIds={selected.map((item) => item.id)}
      />
    );
  }

  return (
    <Select<SystemAudioSource>
      aria-label="System audio"
      className="w-full"
      clearable={false}
      items={items}
      leftSection={<Volume2 size={14} />}
      onChange={(selection) => {
        const match = items.find((item) => item.id === selection);
        if (match) onChange([match]);
      }}
      placeholder="No system audio"
      showFocus={false}
      size="sm"
      value={selected[0]?.id ?? null}
      variant="ghost"
    >
      {(item: SystemAudioSource) => (
        <ListBoxItem id={item.id} textValue={item.label}>
          {item.label}
        </ListBoxItem>
      )}
    </Select>
  );
}

type PermissionOverlayProps = {
  label: string;
  onPress?: () => void;
};

function PermissionOverlay({ label, onPress }: PermissionOverlayProps) {
  return (
    <div className="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-content/80 backdrop-blur-sm">
      <Button
        className="gap-1.5"
        onPress={onPress}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <Lock size={13} />
        {label}
      </Button>
    </div>
  );
}

export type RecordingOptionsProps = {
  audioSources: SystemAudioSource[];
  cameras: CameraDevice[];
  microphones: InputDevice[];
  onCameraChange: (camera: CameraDevice) => void;
  onCameraResolutionChange: (resolution: CameraResolution) => void;
  onMicrophoneChange: (microphone: InputDevice) => void;
  onSystemAudioChange: (sources: SystemAudioSource[]) => void;
  selectedCamera: CameraDevice | null;
  selectedCameraResolution: CameraResolution | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  cameraFlipped?: boolean;
  cameraLocked?: boolean;
  cameraPal?: boolean;
  cameraPreviewActive?: boolean;
  cameraPreviewRef?: RefObject<HTMLCanvasElement | null>;
  /** The preview is wanted but has not drawn a frame yet. */
  cameraPreviewStarting?: boolean;
  microphoneDecibels?: number;
  microphoneLocked?: boolean;
  microphonePeak?: number;
  microphonePreviewEnabled?: boolean;
  onCameraFlippedChange?: (flipped: boolean) => void;
  onCameraLockedPress?: () => void;
  onCameraOptionsOpen?: () => Promise<CameraDevice[]>;
  onCameraPalChange?: (pal: boolean) => void;
  onMicrophoneLockedPress?: () => void;
  onMicrophoneOptionsOpen?: () => Promise<InputDevice[]>;
  onSystemAudioOptionsOpen?: () => Promise<SystemAudioSource[]>;
  standalone?: boolean;
  systemAudioDecibels?: number;
  systemAudioPeak?: number;
  systemAudioPreviewEnabled?: boolean;
};

export function RecordingOptions({
  audioSources,
  cameraFlipped = false,
  cameraLocked = false,
  cameraPal = false,
  cameraPreviewActive = false,
  cameraPreviewRef,
  cameraPreviewStarting = false,
  cameras,
  microphoneDecibels = -Infinity,
  microphoneLocked = false,
  microphonePeak = -Infinity,
  microphonePreviewEnabled = false,
  microphones,
  onCameraChange,
  onCameraFlippedChange,
  onCameraLockedPress,
  onCameraOptionsOpen,
  onCameraPalChange,
  onCameraResolutionChange,
  onMicrophoneChange,
  onMicrophoneLockedPress,
  onMicrophoneOptionsOpen,
  onSystemAudioChange,
  onSystemAudioOptionsOpen,
  selectedCamera,
  selectedCameraResolution,
  selectedMicrophone,
  selectedSystemAudio,
  standalone = false,
  systemAudioDecibels = -Infinity,
  systemAudioPeak = -Infinity,
  systemAudioPreviewEnabled = false,
}: RecordingOptionsProps) {
  const previewIsWiderThanStage =
    selectedCameraResolution !== null &&
    selectedCameraResolution.width * 9 >= selectedCameraResolution.height * 16;

  return (
    <main className="window-surface flex h-full min-h-[300px] w-full min-w-[240px] flex-col gap-3 overflow-hidden rounded-[10px] bg-content/92 p-4 text-content-fg">
      <section className="relative flex flex-col gap-1.5">
        {cameraLocked ? (
          <PermissionOverlay
            label="Grant camera access"
            onPress={onCameraLockedPress}
          />
        ) : null}

        <div className="relative flex aspect-video w-full shrink-0 items-center justify-center text-muted">
          <div className="absolute inset-1 flex min-h-0 min-w-0 items-center justify-center">
            <canvas
              aria-label="Camera preview"
              className={cn(
                "block shrink-0 self-center shadow-[0_2px_12px_rgb(0_0_0/0.38)]",
                previewIsWiderThanStage
                  ? "h-auto max-h-full w-full"
                  : "h-full w-auto max-w-full",
                cameraFlipped && "-scale-x-100",
              )}
              hidden={!cameraPreviewActive}
              ref={cameraPreviewRef}
              role="img"
            />
          </div>
          {!cameraPreviewActive ? (
            cameraPreviewStarting ? (
              <CircularProgressBar
                aria-label="Starting camera preview"
                isIndeterminate
                size={24}
                strokeWidth={12}
              />
            ) : (
              <CameraOff size={24} />
            )
          ) : null}
          {selectedCamera && onCameraFlippedChange ? (
            <ToggleButton
              aria-label="Flip camera horizontally"
              className="absolute right-2 bottom-2"
              isSelected={cameraFlipped}
              onChange={onCameraFlippedChange}
              showFocus={false}
              size="sm"
              variant="ghost"
            >
              <FlipHorizontal2 size={14} />
            </ToggleButton>
          ) : null}
        </div>

        <InputSelect
          icon={<Camera size={14} />}
          id="camera"
          items={cameras}
          label="Camera"
          onChange={onCameraChange}
          onOpen={onCameraOptionsOpen}
          placeholder="No camera"
          selected={selectedCamera}
          standalone={standalone}
        />
        <div className="flex items-center gap-1">
          <div className="min-w-0 flex-1">
            <InputSelect
              icon={<Scan size={14} />}
              id="camera-resolution"
              items={selectedCamera?.modes ?? []}
              label="Camera resolution"
              onChange={onCameraResolutionChange}
              placeholder="No resolution"
              selected={selectedCameraResolution}
              standalone={standalone}
            />
          </div>
          {selectedCamera && onCameraPalChange ? (
            <TooltipTrigger delay={400}>
              <ToggleButton
                aria-label="Anti-flicker"
                isSelected={cameraPal}
                onChange={onCameraPalChange}
                showFocus={false}
                size="sm"
                variant="ghost"
              >
                <span className="text-xxs font-medium tabular-nums">PAL</span>
              </ToggleButton>
              <Tooltip placement="top">Anti-flicker</Tooltip>
            </TooltipTrigger>
          ) : null}
        </div>
      </section>

      <section className="relative flex flex-col gap-1">
        {microphoneLocked ? (
          <PermissionOverlay
            label="Grant microphone access"
            onPress={onMicrophoneLockedPress}
          />
        ) : null}

        <InputSelect
          icon={<Mic size={14} />}
          id="microphone"
          items={microphones}
          label="Microphone"
          onChange={onMicrophoneChange}
          onOpen={onMicrophoneOptionsOpen}
          placeholder="No microphone"
          selected={selectedMicrophone}
          standalone={standalone}
        />
        <AudioMeter
          decibels={microphoneDecibels}
          disabled={!microphonePreviewEnabled}
          height={5}
          hidePeakTick
          hideTicks
          peak={microphonePeak}
          width="100%"
        />
      </section>

      <section className="flex flex-col gap-1">
        <SystemAudioSelect
          items={audioSources}
          onChange={onSystemAudioChange}
          onOpen={onSystemAudioOptionsOpen}
          selected={selectedSystemAudio}
          standalone={standalone}
        />
        <AudioMeter
          decibels={systemAudioDecibels}
          disabled={!systemAudioPreviewEnabled}
          height={5}
          hidePeakTick
          hideTicks
          peak={systemAudioPeak}
          width="100%"
        />
      </section>
    </main>
  );
}
