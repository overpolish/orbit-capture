// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  Camera,
  Check,
  CirclePause,
  CirclePlay,
  CircleStop,
  GripVertical,
  LoaderCircle,
  Trash2,
  X,
} from "lucide-react";
import { useLayoutEffect, useRef } from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { ContentRotate } from "../../../components/base/content-rotate/content-rotate";
import { Overlay } from "../../../components/base/overlay/overlay";
import { ConfirmActionButton } from "../../../components/shared/confirm-action-button/confirm-action-button";
import { cn } from "../../../lib/styling";
import { AudioMeter } from "../../audio-inputs/components/audio-meter";
import { formatElapsedTime } from "../elapsed-time";
import { RecordingStatus } from "../types";
import { RecordingMonitorSnapshot } from "../use-recording-monitor";

const ICON_SIZE = 18;

/**
 * Rotates each digit on its own, so a tick only animates what actually
 * changed: 58 to 59 moves the units alone, while 59 to 00 moves both. Rotating
 * the pair as one unit would swing the tens digit on every single second.
 */
function RotatingDigits({ value }: { value: string }) {
  const leading = value.slice(0, -1);
  const last = value.slice(-1);

  return (
    <>
      <ContentRotate contentKey={leading}>{leading}</ContentRotate>
      <ContentRotate contentKey={last}>{last}</ContentRotate>
    </>
  );
}

type DiscardButtonProps = {
  isDisabled: boolean;
  onDiscard?: () => void;
};

/**
 * Two-step, because discarding sits one button away from stopping: the bin
 * swaps in place to a red check, and only pressing that check discards. The
 * swap is the pause button's, so the three controls stay of a piece.
 */
function DiscardButton({ isDisabled, onDiscard }: DiscardButtonProps) {
  return (
    <ConfirmActionButton
      armedIcon={
        <Check className="text-error" size={ICON_SIZE} strokeWidth={3} />
      }
      armedLabel="Confirm discarding"
      className="h-9 w-9"
      idleIcon={<Trash2 size={ICON_SIZE} />}
      idleLabel="Discard recording"
      isDisabled={isDisabled}
      onConfirm={onDiscard}
    />
  );
}

type RecordingDockProps = {
  countdownSeconds?: number;
  elapsedMs?: number;
  monitor?: RecordingMonitorSnapshot;
  onDiscard?: () => void;
  onPauseChange?: (isPaused: boolean) => void;
  onPointerUp?: () => void;
  onStop?: () => void;
  onWidthChange?: (width: number) => void;
  status?: RecordingStatus;
};

export function RecordingDock({
  countdownSeconds = 0,
  elapsedMs = 0,
  monitor,
  onDiscard,
  onPauseChange,
  onPointerUp,
  onStop,
  onWidthChange,
  status = "recording",
}: RecordingDockProps) {
  const dockRef = useRef<HTMLElement>(null);
  const isBusy = status === "starting" || status === "stopping";
  // Remounting between sessions drops any half-armed discard, so a recording
  // never inherits an armed button from the one before it.
  const sessionKey = status === "idle" ? "idle" : "session";
  const isPaused = status === "paused";
  const isRecording = status === "recording";
  const { hours, minutes, seconds } = formatElapsedTime(elapsedMs);
  const confidenceDisabled = !isRecording;
  const hasConfidenceChecks =
    monitor?.hasCamera === true ||
    monitor?.hasSystemAudio === true ||
    monitor?.hasMicrophone === true;

  useLayoutEffect(() => {
    const dock = dockRef.current;
    if (!dock || !onWidthChange) return;

    const reportWidth = () => {
      onWidthChange(Math.ceil(dock.getBoundingClientRect().width));
    };
    const observer = new ResizeObserver(reportWidth);
    observer.observe(dock);
    reportWidth();

    return () => {
      observer.disconnect();
    };
  }, [onWidthChange]);

  return (
    <main
      className="window-surface relative flex h-full min-h-11 w-max items-center overflow-hidden rounded-[10px] bg-content/92 pr-1 text-content-fg"
      onPointerUpCapture={onPointerUp}
      ref={dockRef}
    >
      <Overlay
        aria-label={
          status === "starting" ? "Starting recording" : "Finishing recording"
        }
        className={`z-60 rounded-[10px] bg-content/70 text-content-fg ${countdownSeconds > 0 ? "" : "gap-2 text-xs font-semibold"}`}
        contained
        isOpen={isBusy}
      >
        {status === "starting" && countdownSeconds > 0 ? (
          <>
            <ContentRotate
              className="absolute inset-0 flex items-center justify-center text-2xl font-semibold tabular-nums"
              contentKey={String(countdownSeconds)}
            >
              {countdownSeconds}
            </ContentRotate>
            <Button
              aria-label="Cancel recording countdown"
              className="absolute right-1 h-9 w-9"
              onPress={onDiscard}
              size="sm"
              variant="ghost"
            >
              <X size={ICON_SIZE} />
            </Button>
          </>
        ) : (
          <div className="absolute inset-0 flex items-center justify-center gap-2 text-xs font-semibold">
            <span className="flex transform-gpu">
              <LoaderCircle
                className="animate-spin text-muted"
                size={ICON_SIZE}
              />
            </span>
            {status === "starting" ? "Starting" : "Finishing"}
          </div>
        )}
        {countdownSeconds === 0 ? (
          <Button
            aria-label={
              status === "starting"
                ? "Cancel starting recording"
                : "Cancel finishing recording"
            }
            className="absolute right-1 h-9 w-9"
            onPress={onDiscard}
            size="sm"
            variant="ghost"
          >
            <X size={ICON_SIZE} />
          </Button>
        ) : null}
      </Overlay>
      <div
        className="flex h-full shrink-0 cursor-grab items-center pl-0.5 text-muted"
        data-tauri-drag-region
      >
        <GripVertical className="pointer-events-none" size={20} />
      </div>
      {hasConfidenceChecks && (
        <div
          className="flex h-full shrink-0 cursor-grab items-center gap-1 py-2 pr-1"
          data-tauri-drag-region
        >
          {monitor.hasCamera && (
            <div className="relative flex h-7 w-10 items-center justify-center overflow-hidden rounded bg-muted/12">
              <canvas
                aria-label="Camera confidence preview"
                className={cn(
                  "pointer-events-none h-full w-full object-cover transition-opacity",
                  (!monitor.hasCameraFrame || confidenceDisabled) &&
                    "opacity-35",
                )}
                ref={monitor.cameraCanvasRef}
              />
              {!monitor.hasCameraFrame && (
                <Camera className="absolute text-muted" size={12} />
              )}
            </div>
          )}
          {(monitor.hasSystemAudio || monitor.hasMicrophone) && (
            <div className="flex gap-0.5">
              {monitor.hasSystemAudio && (
                <AudioMeter
                  decibels={monitor.systemAudioDecibels}
                  disabled={confidenceDisabled}
                  height={28}
                  hidePeakTick
                  hideTicks
                  orientation="vertical"
                  width={4}
                />
              )}
              {monitor.hasMicrophone && (
                <AudioMeter
                  decibels={monitor.microphoneDecibels}
                  disabled={confidenceDisabled}
                  height={28}
                  hidePeakTick
                  hideTicks
                  orientation="vertical"
                  width={4}
                />
              )}
            </div>
          )}
        </div>
      )}

      <div
        className="flex w-[64px] cursor-grab justify-center text-xs font-semibold tabular-nums"
        data-tauri-drag-region
      >
        <div className={cn("flex transition-colors", isPaused && "text-muted")}>
          <RotatingDigits value={hours} />:<RotatingDigits value={minutes} />:
          <RotatingDigits value={seconds} />
        </div>
      </div>

      <ToggleButton
        aria-label={isPaused ? "Resume recording" : "Pause recording"}
        className="h-9 w-9"
        isDisabled={isBusy}
        isSelected={isPaused}
        off={<CirclePause size={ICON_SIZE} />}
        onChange={(selected) => {
          onPauseChange?.(selected);
        }}
        variant="ghost"
      >
        <CirclePlay
          className={cn(
            "transition-colors",
            isPaused && "animate-pulse text-warning",
          )}
          size={ICON_SIZE}
        />
      </ToggleButton>

      <Button
        aria-label="Stop recording"
        className="cursor-default"
        icon
        isDisabled={isBusy}
        onPress={onStop}
        showFocus={false}
        variant="ghost"
      >
        <CircleStop
          className={cn(
            "transition-colors",
            isRecording && "animate-pulse text-error",
          )}
          size={ICON_SIZE}
        />
      </Button>

      <DiscardButton
        isDisabled={isBusy}
        key={sessionKey}
        onDiscard={onDiscard}
      />
    </main>
  );
}
