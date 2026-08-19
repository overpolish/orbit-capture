// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Camera,
  CameraOff,
  Check,
  Circle,
  CircleX,
  ClipboardCopy,
  ImageDown,
  Lock,
  Mic,
  MicOff,
  MousePointer2,
  MousePointer2Off,
  Sparkle,
  Volume2,
  VolumeOff,
} from "lucide-react";
import { useRef, useState } from "react";

import { Button } from "../../../components/base/button/button";
import { Overlay } from "../../../components/base/overlay/overlay";
import { Separator } from "../../../components/base/separator/separator";
import { Sparkles } from "../../../components/base/sparkles/sparkles";
import { cn } from "../../../lib/styling";
import { RecordingFps, RecordingInputs } from "../../recording-inputs/types";
import { RecordingMode } from "../../recording-sources/types";
import { canStartRecording } from "../can-record";
import { RecordingStatus, ScreenshotAction, ScreenshotState } from "../types";

import { RecordingBarInputToggle as InputToggle } from "./recording-bar-input-toggle";
import { RecordingModePicker } from "./recording-mode-picker";

type RecordingBarProps = {
  fps?: RecordingFps;
  hasCameraWarning?: boolean;
  hasMicrophoneWarning?: boolean;
  hasSelectedMonitor?: boolean;
  hasSelectedWindow?: boolean;
  hasSystemAudioWarning?: boolean;
  initialFps?: RecordingFps;
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
  onFocusPendingExport?: () => void;
  onFpsChange?: (fps: RecordingFps) => void;
  onInputChange?: (input: keyof RecordingInputs, selected: boolean) => void;
  onInteract?: () => void;
  onMicrophoneLockedPress?: () => void;
  onModeChange?: (mode: RecordingMode) => void;
  onOptions?: (anchorX: number) => void;
  onPointerUp?: () => void;
  onRecord?: () => void;
  onScreenshot?: () => void;
  onScreenshotToClipboard?: () => void;
  /**
   * Which workspaces are holding unsaved work. Each has a window of its own,
   * so only a pending recording stands in the way of starting another.
   */
  pendingExports?: { recording: boolean; screenshot: boolean };
  screenshotAction?: ScreenshotAction;
  screenshotState?: ScreenshotState;
  status?: RecordingStatus;
};

const defaultInputs: RecordingInputs = {
  camera: false,
  microphone: false,
  showCursor: true,
  systemAudio: false,
};

export function RecordingBar({
  fps: controlledFps,
  hasCameraWarning = false,
  hasMicrophoneWarning = false,
  hasSelectedMonitor = false,
  hasSelectedWindow = false,
  hasSystemAudioWarning = false,
  initialFps = 60,
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
  onFocusPendingExport,
  onFpsChange,
  onInputChange,
  onInteract,
  onMicrophoneLockedPress,
  onModeChange,
  onOptions,
  onPointerUp,
  onRecord,
  onScreenshot,
  onScreenshotToClipboard,
  pendingExports = { recording: false, screenshot: false },
  screenshotAction = "export",
  screenshotState = "idle",
  status = "idle",
}: RecordingBarProps) {
  const [uncontrolledMode, setUncontrolledMode] =
    useState<RecordingMode>(initialMode);
  const [uncontrolledFps, setUncontrolledFps] =
    useState<RecordingFps>(initialFps);
  const [uncontrolledInputs, setUncontrolledInputs] = useState<RecordingInputs>(
    {
      ...defaultInputs,
      ...initialInputs,
    },
  );
  const optionsButtonRef = useRef<HTMLButtonElement>(null);

  const mode = controlledMode ?? uncontrolledMode;
  const fps = controlledFps ?? uncontrolledFps;
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
  const isRecordingWorkspaceOpen = pendingExports.recording;
  const isCapturingStill = screenshotState === "pending";
  const exportScreenshotState =
    screenshotAction === "export" ? screenshotState : "idle";
  const clipboardScreenshotState =
    screenshotAction === "clipboard" ? screenshotState : "idle";
  const canCaptureStill =
    isScreenCapture && !isScreenshotLocked && !isRecordingActive;
  // A pending recording no longer stands in a screenshot's way: it waits in
  // its own window while the screenshot workspace opens beside it.
  const canExportScreenshot = canCaptureStill;
  const canCopyScreenshot = canCaptureStill;
  const canRecordIgnoringExport =
    !isRecordingActive &&
    canStartRecording({
      hasCameraWarning,
      hasMicrophoneWarning,
      hasSelectedMonitor,
      hasSelectedWindow,
      hasSystemAudioWarning,
      inputs,
      isCameraLocked: Boolean(isCameraLocked),
      isMicrophoneLocked: Boolean(isMicrophoneLocked),
      isScreenLocked: Boolean(isLocked),
      mode,
    });
  const canRecord = canRecordIgnoringExport && !isRecordingWorkspaceOpen;
  // Recording that only the pending recording stands in the way of: the button
  // stays pressable and brings that export forward rather than going dead,
  // which is the same escape hatch the global shortcuts take.
  const isRecordBlockedByExport =
    canRecordIgnoringExport && isRecordingWorkspaceOpen;

  return (
    <main
      className="window-surface flex h-full min-h-[92px] w-full min-w-[648px] items-center justify-center overflow-hidden rounded-[10px] bg-content/92 p-2 text-content-fg"
      data-tauri-drag-region="deep"
      onKeyDownCapture={(event) => {
        if (optionsButtonRef.current?.contains(event.target as Node)) return;
        onInteract?.();
      }}
      onPointerDownCapture={(event) => {
        if (optionsButtonRef.current?.contains(event.target as Node)) return;
        onInteract?.();
      }}
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
          <CircleX className="origin-center transform-gpu backface-hidden text-muted will-change-transform transition-[color,transform,scale] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg" />
        </div>
      </Button>

      <Separator className="h-[60px]" orientation="vertical" spacing="sm" />

      <RecordingModePicker
        isDisabled={isRecordingActive}
        mode={mode}
        onChange={(nextMode) => {
          if (controlledMode === undefined) {
            setUncontrolledMode(nextMode);
          }
          onModeChange?.(nextMode);
        }}
      />

      <Separator className="h-[60px]" orientation="vertical" spacing="sm" />

      <div className="mr-2 flex min-w-24 flex-col">
        <div className="flex justify-between px-2">
          <InputToggle
            hasWarning={hasSystemAudioWarning}
            isDisabled={isRecordingActive}
            isSelected={inputs.systemAudio}
            label="System audio"
            off={<VolumeOff size={16} />}
            on={<Volume2 size={16} />}
            onChange={(selected) => {
              setInput("systemAudio", selected);
            }}
            warningLabel="One or more selected system audio applications are not detected"
          />
          <InputToggle
            hasWarning={hasMicrophoneWarning}
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
            warningLabel="Selected microphone is not detected"
          />
          <InputToggle
            hasWarning={hasCameraWarning}
            isDisabled={isAudioOnly || isRecordingActive}
            isLocked={isCameraLocked}
            isReadOnly={mode === "camera"}
            isSelected={mode === "camera" || (!isAudioOnly && inputs.camera)}
            label="Camera"
            off={<CameraOff size={16} />}
            on={<Camera size={16} />}
            onChange={(selected) => {
              setInput("camera", selected);
            }}
            onLockedPress={onCameraLockedPress}
            warningLabel="Selected camera is not detected"
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

      {/* `mr-3` rather than a gap on the row: everything to the left of here
          is separated by rules, and only these last two columns - which grew a
          toggle each underneath them - need air between them. */}
      <div className="mr-3 flex flex-col items-center justify-center self-stretch">
        <Button
          aria-label="Take screenshot"
          className="group cursor-default p-1"
          isDisabled={!canExportScreenshot || isCapturingStill}
          onPress={onScreenshot}
          showFocus={false}
          variant="ghost"
        >
          {exportScreenshotState === "done" ? (
            <Check className="text-success" size={40} strokeWidth={3} />
          ) : (
            <ImageDown
              className={cn(
                "origin-center transform-gpu backface-hidden will-change-transform transition-[color,transform,scale] group-data-[hovered]:scale-110",
                isCapturingStill && "animate-pulse text-muted",
                exportScreenshotState === "failed" && "text-error",
              )}
              size={40}
            />
          )}
        </Button>

        <Button
          aria-label="Copy screenshot to clipboard"
          className="group cursor-default"
          isDisabled={!canCopyScreenshot || isCapturingStill}
          onPress={onScreenshotToClipboard}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          {clipboardScreenshotState === "done" ? (
            <Check className="text-success" size={16} strokeWidth={3} />
          ) : (
            <ClipboardCopy
              className={cn(
                "origin-center transform-gpu backface-hidden will-change-transform transition-[color,transform,scale] group-data-[hovered]:scale-110",
                isCapturingStill && "animate-pulse text-muted",
                clipboardScreenshotState === "failed" && "text-error",
              )}
              size={16}
            />
          )}
        </Button>
      </div>

      <Sparkles
        icon={Sparkle}
        offset={{ x: { max: 70, min: 0 }, y: { max: 50, min: -10 } }}
        scale={{ max: 0.5, min: 0.2 }}
        sparklesCount={canRecord ? 2 : 0}
      >
        {/* Padding rather than margin: this sits inside the sparkle wrapper's
            inline-block, so the space has to come from within it to widen the
            column at all. */}
        <div className="flex flex-col items-center justify-center self-stretch pr-2">
          <Button
            aria-label={
              isRecordBlockedByExport ? "Show export window" : "Start recording"
            }
            className="group cursor-default p-1"
            isDisabled={!canRecord && !isRecordBlockedByExport}
            onPress={isRecordBlockedByExport ? onFocusPendingExport : onRecord}
            showFocus={false}
            variant="ghost"
          >
            <Circle
              className={cn(
                "origin-center transform-gpu backface-hidden will-change-transform transition-[color,transform,scale]",
                !isRecordBlockedByExport && "group-data-[hovered]:scale-110",
                isRecordBlockedByExport &&
                  "text-muted group-data-[hovered]:text-content-fg/75",
              )}
              size={40}
            />
          </Button>

          <InputToggle
            isDisabled={isRecordingActive}
            isSelected={fps === 60}
            label="Frames per second"
            off={<span className="text-xxs tabular-nums">30</span>}
            on={<span className="text-xxs tabular-nums">60</span>}
            onChange={(smooth) => {
              const nextFps: RecordingFps = smooth ? 60 : 30;
              if (controlledFps === undefined) setUncontrolledFps(nextFps);
              onFpsChange?.(nextFps);
            }}
          />
        </div>
      </Sparkles>
    </main>
  );
}
