import {
  AppWindowMac,
  AudioLines,
  Camera,
  CameraOff,
  Check,
  Circle,
  CircleX,
  FolderDown,
  ImageDown,
  Images,
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
import { ReactNode, useRef, useState } from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Overlay } from "../../../components/base/overlay/overlay";
import { RadioGroup } from "../../../components/base/radio-group/radio-group";
import { Separator } from "../../../components/base/separator/separator";
import { Sparkles } from "../../../components/base/sparkles/sparkles";
import { cn } from "../../../lib/styling";
import { RecordingInputs } from "../../recording-inputs/types";
import { RecordingMode } from "../../recording-sources/types";
import { canStartRecording } from "../can-record";
import { RecordingStatus, ScreenshotState } from "../types";

import { IconRadio } from "./icon-radio";

type RecordingBarProps = {
  hasSelectedMonitor?: boolean;
  hasSelectedWindow?: boolean;
  initialInputs?: Partial<RecordingInputs>;
  initialMode?: RecordingMode;
  inputs?: RecordingInputs;
  isCameraLocked?: boolean;
  isLocked?: boolean;
  isMicrophoneLocked?: boolean;
  isScreenshotLocked?: boolean;
  mode?: RecordingMode;
  onCameraLockedPress?: () => void;
  onCancel?: () => void;
  onInputChange?: (input: keyof RecordingInputs, selected: boolean) => void;
  onInteract?: () => void;
  onMicrophoneLockedPress?: () => void;
  onModeChange?: (mode: RecordingMode) => void;
  onOptions?: (anchorX: number) => void;
  onPointerUp?: () => void;
  onRecord?: () => void;
  onScreenshot?: () => void;
  onScreenshotToClipboardChange?: (toClipboard: boolean) => void;
  screenshotState?: ScreenshotState;
  screenshotToClipboard?: boolean;
  status?: RecordingStatus;
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
  hasSelectedMonitor = false,
  hasSelectedWindow = false,
  initialInputs,
  initialMode = "screen",
  inputs: controlledInputs,
  isCameraLocked,
  isLocked,
  isMicrophoneLocked,
  isScreenshotLocked,
  mode: controlledMode,
  onCameraLockedPress,
  onCancel,
  onInputChange,
  onInteract,
  onMicrophoneLockedPress,
  onModeChange,
  onOptions,
  onPointerUp,
  onRecord,
  onScreenshot,
  onScreenshotToClipboardChange,
  screenshotState = "idle",
  screenshotToClipboard = true,
  status = "idle",
}: RecordingBarProps) {
  const [uncontrolledMode, setUncontrolledMode] =
    useState<RecordingMode>(initialMode);
  const [uncontrolledInputs, setUncontrolledInputs] = useState<RecordingInputs>(
    {
      ...defaultInputs,
      ...initialInputs,
    },
  );
  const optionsButtonRef = useRef<HTMLButtonElement>(null);

  const mode = controlledMode ?? uncontrolledMode;
  const inputs = controlledInputs ?? uncontrolledInputs;

  const setInput = (input: keyof RecordingInputs, selected: boolean) => {
    if (controlledInputs === undefined) {
      setUncontrolledInputs((current) => ({
        ...current,
        [input]: selected,
      }));
    }
    onInputChange?.(input, selected);
  };

  const isAudioOnly = mode === "audio";
  const isScreenCapture = ["screen", "region", "window"].includes(mode);
  // The bar is hidden by Rust while a recording runs; disabling it as well
  // keeps a stale window from starting a second one.
  const isRecordingActive = status !== "idle";
  const isCapturingStill = screenshotState === "pending";
  const canScreenshot =
    isScreenCapture && !isScreenshotLocked && !isRecordingActive;
  const canRecord =
    !isRecordingActive &&
    canStartRecording({
      hasSelectedMonitor,
      hasSelectedWindow,
      inputs,
      isCameraLocked: Boolean(isCameraLocked),
      isMicrophoneLocked: Boolean(isMicrophoneLocked),
      isScreenLocked: Boolean(isLocked),
      mode,
    });

  return (
    <main
      className="window-surface flex h-full min-h-[92px] w-full min-w-[628px] items-center justify-center overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg"
      data-tauri-drag-region="deep"
      onPointerDown={onInteract}
      onPointerUpCapture={onPointerUp}
    >
      <Overlay
        blur="sm"
        className="rounded-[10px]"
        isOpen={Boolean(isScreenshotLocked) && isScreenCapture}
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
        isDisabled={isRecordingActive}
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
            isDisabled={isRecordingActive}
            isSelected={inputs.systemAudio}
            label="System audio"
            off={<VolumeOff size={16} />}
            on={<Volume2 size={16} />}
            onChange={(selected) => {
              setInput("systemAudio", selected);
            }}
          />
          <InputToggle
            isDisabled={isRecordingActive}
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
            isDisabled={isAudioOnly || isRecordingActive}
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
            isDisabled={!isScreenCapture || isRecordingActive}
            isSelected={isScreenCapture && inputs.showCursor}
            label="Show cursor"
            off={<MousePointer2Off size={16} />}
            on={<MousePointer2 size={16} />}
            onChange={(selected) => {
              setInput("showCursor", selected);
            }}
          />
        </div>

        <div
          className="flex justify-center"
          onPointerDown={(event) => {
            event.stopPropagation();
          }}
        >
          <Button
            className="origin-center transform-gpu backface-hidden justify-center will-change-transform transition-transform data-[hovered]:scale-110"
            isDisabled={isRecordingActive}
            onPress={() => {
              const bounds = optionsButtonRef.current?.getBoundingClientRect();
              if (bounds) onOptions?.(bounds.left + bounds.width / 2);
            }}
            ref={optionsButtonRef}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            Options
          </Button>
        </div>
      </div>

      <div className="flex flex-col items-center justify-center self-stretch">
        <Button
          aria-label="Take screenshot"
          className="group cursor-default p-1"
          isDisabled={!canScreenshot || isCapturingStill}
          onPress={onScreenshot}
          showFocus={false}
          variant="ghost"
        >
          {screenshotState === "done" ? (
            <Check className="text-success" size={40} strokeWidth={3} />
          ) : (
            <ImageDown
              className={cn(
                "origin-center transform-gpu backface-hidden will-change-transform transition-[color,transform] group-data-[hovered]:scale-110",
                isCapturingStill && "animate-pulse text-muted",
                screenshotState === "failed" && "text-error",
              )}
              size={40}
            />
          )}
        </Button>

        <InputToggle
          isDisabled={!canScreenshot}
          isSelected={screenshotToClipboard}
          label="Copy screenshot to clipboard"
          off={<FolderDown size={16} />}
          on={<Images size={16} />}
          onChange={(selected) => {
            onScreenshotToClipboardChange?.(selected);
          }}
        />
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
          </div>
        </Button>
      </Sparkles>
    </main>
  );
}
