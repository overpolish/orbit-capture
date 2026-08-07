import { Camera, CameraOff, Lock, Mic, Volume2 } from "lucide-react";
import { RefObject } from "react";

import { Button } from "../../components/base/button/button";
import { ListBoxItem } from "../../components/base/listbox-item/listbox-item";
import { Select } from "../../components/base/select/select";
import { AudioMeter } from "../audio-inputs/components/audio-meter";
import { StandaloneMultiSelect } from "../standalone-listbox/standalone-multi-select";
import { StandaloneSelect } from "../standalone-listbox/standalone-select";

import { InputDevice, SystemAudioSource } from "./types";

type InputSelectProps<T extends InputDevice> = {
  icon: React.ReactNode;
  id: string;
  items: T[];
  label: string;
  onChange: (item: T) => void;
  placeholder: string;
  selected: T | null;
  standalone: boolean;
};

function InputSelect<T extends InputDevice>({
  icon,
  id,
  items,
  label,
  onChange,
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
};

function SystemAudioSelect({
  items,
  onChange,
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
  cameras: InputDevice[];
  microphones: InputDevice[];
  onCameraChange: (camera: InputDevice) => void;
  onMicrophoneChange: (microphone: InputDevice) => void;
  onSystemAudioChange: (sources: SystemAudioSource[]) => void;
  selectedCamera: InputDevice | null;
  selectedMicrophone: InputDevice | null;
  selectedSystemAudio: SystemAudioSource[];
  cameraEnabled?: boolean;
  cameraLocked?: boolean;
  cameraPreviewActive?: boolean;
  cameraPreviewRef?: RefObject<HTMLCanvasElement | null>;
  microphoneDecibels?: number;
  microphoneEnabled?: boolean;
  microphoneLocked?: boolean;
  microphonePeak?: number;
  onCameraLockedPress?: () => void;
  onMicrophoneLockedPress?: () => void;
  standalone?: boolean;
  systemAudioDecibels?: number;
  systemAudioEnabled?: boolean;
  systemAudioPeak?: number;
};

export function RecordingOptions({
  audioSources,
  cameraEnabled = false,
  cameraLocked = false,
  cameraPreviewActive = false,
  cameraPreviewRef,
  cameras,
  microphoneDecibels = -Infinity,
  microphoneEnabled = false,
  microphoneLocked = false,
  microphonePeak = -Infinity,
  microphones,
  onCameraChange,
  onCameraLockedPress,
  onMicrophoneChange,
  onMicrophoneLockedPress,
  onSystemAudioChange,
  selectedCamera,
  selectedMicrophone,
  selectedSystemAudio,
  standalone = false,
  systemAudioDecibels = -Infinity,
  systemAudioEnabled = false,
  systemAudioPeak = -Infinity,
}: RecordingOptionsProps) {
  return (
    <main className="flex h-full min-h-[270px] w-full min-w-[240px] flex-col gap-3 overflow-hidden rounded-[10px] bg-content/92 p-4 text-content-fg">
      <section className="relative flex flex-col gap-1.5">
        {cameraLocked ? (
          <PermissionOverlay
            label="Grant camera access"
            onPress={onCameraLockedPress}
          />
        ) : null}

        <div className="relative flex aspect-video items-center justify-center overflow-hidden rounded-md bg-content-fg/10 text-muted shadow-sm">
          <canvas
            aria-label="Camera preview"
            className="h-full w-full -scale-x-100 object-cover"
            hidden={!cameraPreviewActive || !cameraEnabled}
            ref={cameraPreviewRef}
            role="img"
          />
          {!cameraPreviewActive || !cameraEnabled ? (
            <CameraOff size={24} />
          ) : null}
        </div>

        <InputSelect
          icon={<Camera size={14} />}
          id="camera"
          items={cameras}
          label="Camera"
          onChange={onCameraChange}
          placeholder="No camera"
          selected={selectedCamera}
          standalone={standalone}
        />
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
          placeholder="No microphone"
          selected={selectedMicrophone}
          standalone={standalone}
        />
        <AudioMeter
          decibels={microphoneDecibels}
          disabled={!microphoneEnabled}
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
          selected={selectedSystemAudio}
          standalone={standalone}
        />
        <AudioMeter
          decibels={systemAudioDecibels}
          disabled={!systemAudioEnabled}
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
