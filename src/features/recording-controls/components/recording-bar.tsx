import {
  AppWindowMac,
  AudioLines,
  Camera,
  CameraOff,
  Circle,
  CircleX,
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
import { ComponentProps, ReactNode, useState } from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Keyboard } from "../../../components/base/keyboard/keyboard";
import { Overlay } from "../../../components/base/overlay/overlay";
import { RadioGroup } from "../../../components/base/radio-group/radio-group";
import { Separator } from "../../../components/base/separator/separator";
import { Sparkles } from "../../../components/base/sparkles/sparkles";

import { IconRadio } from "./icon-radio";

export type RecordingMode = "screen" | "region" | "window" | "audio";

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
  onCancel?: () => void;
  onOptions?: () => void;
  onRecord?: () => void;
};

const keyboardStyle: ComponentProps<typeof Keyboard> = {
  size: "xs",
  variant: "ghost",
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
  onCancel,
  onOptions,
  onRecord,
}: RecordingBarProps) {
  const [mode, setMode] = useState<RecordingMode>(initialMode);
  const [inputs, setInputs] = useState<RecordingInputs>({
    ...defaultInputs,
    ...initialInputs,
  });

  const setInput = (input: keyof RecordingInputs, selected: boolean) => {
    setInputs((current) => ({ ...current, [input]: selected }));
  };

  const isAudioOnly = mode === "audio";
  const canRecord =
    !isAudioOnly ||
    inputs.systemAudio ||
    (inputs.microphone && !isMicrophoneLocked);

  return (
    <main
      className="fixed inset-0 flex items-center justify-center overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg"
      data-tauri-drag-region="deep"
    >
      <Overlay blur="sm" className="rounded-[10px]" isOpen={isLocked}>
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
          <Keyboard {...keyboardStyle}>Esc</Keyboard>
        </div>
      </Button>

      <Separator className="h-[60px]" orientation="vertical" spacing="sm" />

      <RadioGroup
        aria-label="Recording type"
        className="min-w-0 grow"
        onChange={(value) => {
          setMode(value as RecordingMode);
        }}
        orientation="horizontal"
        value={mode}
      >
        <IconRadio
          aria-label="Screen"
          icon={<Monitor size={30} />}
          shortcut={<Keyboard {...keyboardStyle}>1</Keyboard>}
          subtext="Screen"
          value="screen"
        />
        <IconRadio
          aria-label="Region"
          icon={<SquareDashed size={30} />}
          shortcut={<Keyboard {...keyboardStyle}>2</Keyboard>}
          subtext="Region"
          value="region"
        />
        <IconRadio
          aria-label="Window"
          icon={<AppWindowMac size={30} />}
          shortcut={<Keyboard {...keyboardStyle}>3</Keyboard>}
          subtext="Window"
          value="window"
        />
        <IconRadio
          aria-label="Audio only"
          icon={<AudioLines size={30} />}
          shortcut={<Keyboard {...keyboardStyle}>4</Keyboard>}
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
            onLockedPress={onOptions}
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
            onLockedPress={onOptions}
          />
          <InputToggle
            isDisabled={isAudioOnly}
            isSelected={!isAudioOnly && inputs.showCursor}
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
            <Keyboard {...keyboardStyle}>Enter</Keyboard>
          </div>
        </Button>
      </Sparkles>
    </main>
  );
}
