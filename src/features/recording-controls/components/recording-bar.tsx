import {
  AppWindowMac,
  AudioLines,
  Camera,
  CameraOff,
  Circle,
  CircleX,
  ImageDown,
  Lock,
  Mic,
  MicOff,
  Monitor,
  MousePointer2,
  MousePointer2Off,
  Sparkle,
  SquareDashed,
  Volume2,
  VolumeOff,
} from "lucide-react";
import { ReactNode, useState } from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Overlay } from "../../../components/base/overlay/overlay";
import { RadioGroup } from "../../../components/base/radio-group/radio-group";
import { Separator } from "../../../components/base/separator/separator";
import { Sparkles } from "../../../components/base/sparkles/sparkles";
import { RecordingMode } from "../../recording-sources/types";

import { IconRadio } from "./icon-radio";

export type RecordingInputs = {
  camera: boolean;
  microphone: boolean;
  showCursor: boolean;
  systemAudio: boolean;
};

type RecordingBarProps = {
  initialInputs?: Partial<RecordingInputs>;
  initialMode?: RecordingMode;
  isCameraLocked?: boolean;
  isLocked?: boolean;
  isMicrophoneLocked?: boolean;
  mode?: RecordingMode;
  onCameraLockedPress?: () => void;
  onCancel?: () => void;
  onInteract?: () => void;
  onMicrophoneLockedPress?: () => void;
  onModeChange?: (mode: RecordingMode) => void;
  onOptions?: () => void;
  onPointerUp?: () => void;
  onRecord?: () => void;
  onScreenshot?: () => void;
};

const defaultInputs: RecordingInputs = {
  camera: false,
  microphone: false,
  showCursor: true,
  systemAudio: false,
};

type InputToggleProps = {
  isSelected: boolean;
  label: string;
  off: ReactNode;
  on: ReactNode;
  onChange: (isSelected: boolean) => void;
  isDisabled?: boolean;
  isLocked?: boolean;
  onLockedPress?: () => void;
};

function InputToggle({
  isDisabled,
  isLocked,
  isSelected,
  label,
  off,
  on,
  onChange,
  onLockedPress,
}: InputToggleProps) {
  return (
    <div className="relative flex justify-center">
      {isLocked && !isDisabled ? (
        <Lock className="absolute -top-3 text-muted" size={12} />
      ) : null}
      <ToggleButton
        aria-label={label}
        className="data-[disabled]:opacity-35"
        isDisabled={isDisabled}
        isSelected={isSelected}
        off={off}
        onChange={(selected) => {
          if (isLocked) {
            onLockedPress?.();
          } else {
            onChange(selected);
          }
        }}
        size="sm"
        variant="ghost"
      >
        {on}
      </ToggleButton>
    </div>
  );
}

export function RecordingBar({
  initialInputs,
  initialMode = "screen",
  isCameraLocked,
  isLocked,
  isMicrophoneLocked,
  mode: controlledMode,
  onCameraLockedPress,
  onCancel,
  onInteract,
  onMicrophoneLockedPress,
  onModeChange,
  onOptions,
  onPointerUp,
  onRecord,
  onScreenshot,
}: RecordingBarProps) {
  const [uncontrolledMode, setUncontrolledMode] =
    useState<RecordingMode>(initialMode);
  const [inputs, setInputs] = useState<RecordingInputs>({
    ...defaultInputs,
    ...initialInputs,
  });

  const mode = controlledMode ?? uncontrolledMode;

  const setInput = (input: keyof RecordingInputs, selected: boolean) => {
    setInputs((current) => ({ ...current, [input]: selected }));
  };

  const isAudioOnly = mode === "audio";
  const isCameraOnly = mode === "camera";
  const isScreenCapture = ["screen", "region", "window"].includes(mode);
  const canRecord = isAudioOnly
    ? inputs.systemAudio || (inputs.microphone && !isMicrophoneLocked)
    : !isCameraOnly || (inputs.camera && !isCameraLocked);

  return (
    <main
      className="flex h-full min-h-[92px] w-full min-w-[628px] items-center justify-center overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg"
      data-tauri-drag-region="deep"
      onPointerDownCapture={onInteract}
      onPointerUpCapture={onPointerUp}
    >
      <Overlay
        blur="sm"
        className="rounded-[10px]"
        isOpen={isLocked && isScreenCapture}
      >
        <Lock />
      </Overlay>

      <Button
        className="group self-stretch cursor-default"
        onPress={onCancel}
        showFocus={false}
        variant="ghost"
      >
        <div className="flex flex-col items-center gap-1">
          <CircleX className="origin-center transform-gpu backface-hidden text-muted will-change-transform transition-[color,transform] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg" />
        </div>
      </Button>

      <Separator className="h-[60px]" orientation="vertical" spacing="sm" />

      <RadioGroup
        aria-label="Recording type"
        className="min-w-0 grow pr-2"
        onChange={(value) => {
          const nextMode = value as RecordingMode;
          if (controlledMode === undefined) {
            setUncontrolledMode(nextMode);
          }
          onModeChange?.(nextMode);
        }}
        orientation="horizontal"
        value={mode}
      >
        <IconRadio
          aria-label="Screen"
          icon={<Monitor size={30} />}
          subtext="Screen"
          value="screen"
        />
        <IconRadio
          aria-label="Region"
          icon={<SquareDashed size={30} />}
          subtext="Region"
          value="region"
        />
        <IconRadio
          aria-label="Window"
          icon={<AppWindowMac size={30} />}
          subtext="Window"
          value="window"
        />
        <IconRadio
          aria-label="Camera only"
          icon={<Camera size={30} />}
          subtext="Camera"
          value="camera"
        />
        <IconRadio
          aria-label="Audio only"
          icon={<AudioLines size={30} />}
          subtext="Audio"
          value="audio"
        />
      </RadioGroup>

      <Separator className="h-[60px]" orientation="vertical" spacing="sm" />

      <div className="mr-2 flex min-w-24 flex-col">
        <div className="flex justify-between px-2">
          <InputToggle
            isSelected={inputs.systemAudio}
            label="System audio"
            off={<VolumeOff size={16} />}
            on={<Volume2 size={16} />}
            onChange={(selected) => {
              setInput("systemAudio", selected);
            }}
          />
          <InputToggle
            isLocked={isMicrophoneLocked}
            isSelected={inputs.microphone}
            label="Microphone"
            off={<MicOff size={16} />}
            on={<Mic size={16} />}
            onChange={(selected) => {
              setInput("microphone", selected);
            }}
            onLockedPress={onMicrophoneLockedPress}
          />
          <InputToggle
            isDisabled={isAudioOnly}
            isLocked={isCameraLocked}
            isSelected={!isAudioOnly && inputs.camera}
            label="Camera"
            off={<CameraOff size={16} />}
            on={<Camera size={16} />}
            onChange={(selected) => {
              setInput("camera", selected);
            }}
            onLockedPress={onCameraLockedPress}
          />
          <InputToggle
            isDisabled={!isScreenCapture}
            isSelected={isScreenCapture && inputs.showCursor}
            label="Show cursor"
            off={<MousePointer2Off size={16} />}
            on={<MousePointer2 size={16} />}
            onChange={(selected) => {
              setInput("showCursor", selected);
            }}
          />
        </div>

        <Button
          className="origin-center transform-gpu backface-hidden justify-center will-change-transform transition-transform data-[hovered]:scale-110"
          onPress={onOptions}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          Options
        </Button>
      </div>

      <Button
        aria-label="Take screenshot"
        className="group self-stretch cursor-default"
        isDisabled={!isScreenCapture || isLocked}
        onPress={onScreenshot}
        showFocus={false}
        variant="ghost"
      >
        <ImageDown
          className="origin-center transform-gpu backface-hidden will-change-transform transition-transform group-data-[hovered]:scale-110"
          size={40}
        />
      </Button>

      <Sparkles
        icon={Sparkle}
        offset={{ x: { max: 70, min: 0 }, y: { max: 50, min: -10 } }}
        scale={{ max: 0.5, min: 0.2 }}
        sparklesCount={canRecord ? 2 : 0}
      >
        <Button
          aria-label="Start recording"
          className="group self-stretch cursor-default"
          isDisabled={!canRecord}
          onPress={onRecord}
          showFocus={false}
          variant="ghost"
        >
          <div className="flex flex-col items-center gap-1">
            <Circle
              className="origin-center transform-gpu backface-hidden will-change-transform transition-transform group-data-[hovered]:scale-110"
              size={40}
            />
          </div>
        </Button>
      </Sparkles>
    </main>
  );
}
