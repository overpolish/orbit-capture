import {
  Check,
  CirclePause,
  CirclePlay,
  CircleStop,
  GripVertical,
  LoaderCircle,
  Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { ContentRotate } from "../../../components/base/content-rotate/content-rotate";
import { cn } from "../../../lib/styling";
import { formatElapsedTime } from "../elapsed-time";
import { RecordingStatus } from "../types";

const ICON_SIZE = 18;
const DISCARD_CONFIRM_TIMEOUT_MS = 2000;

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
  const [isArmed, setIsArmed] = useState(false);
  const disarmRef = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      window.clearTimeout(disarmRef.current);
    },
    [],
  );

  return (
    <ToggleButton
      aria-label={isArmed ? "Confirm discarding" : "Discard recording"}
      className="h-9 w-9"
      isDisabled={isDisabled}
      isSelected={isArmed}
      off={<Trash2 size={ICON_SIZE} />}
      onChange={(selected) => {
        window.clearTimeout(disarmRef.current);

        if (!selected) {
          // The bin was already armed, so this press is the confirmation.
          setIsArmed(false);
          onDiscard?.();
          return;
        }

        setIsArmed(true);
        disarmRef.current = window.setTimeout(() => {
          setIsArmed(false);
        }, DISCARD_CONFIRM_TIMEOUT_MS);
      }}
      variant="ghost"
    >
      <Check className="text-error" size={ICON_SIZE} strokeWidth={3} />
    </ToggleButton>
  );
}

type RecordingDockProps = {
  elapsedMs?: number;
  onDiscard?: () => void;
  onPauseChange?: (isPaused: boolean) => void;
  onPointerUp?: () => void;
  onStop?: () => void;
  status?: RecordingStatus;
};

export function RecordingDock({
  elapsedMs = 0,
  onDiscard,
  onPauseChange,
  onPointerUp,
  onStop,
  status = "recording",
}: RecordingDockProps) {
  const isBusy = status === "starting" || status === "stopping";
  // Remounting between sessions drops any half-armed discard, so a recording
  // never inherits an armed button from the one before it.
  const sessionKey = status === "idle" ? "idle" : "session";
  const isPaused = status === "paused";
  const isRecording = status === "recording";
  const { hours, minutes, seconds } = formatElapsedTime(elapsedMs);

  return (
    <main
      className="window-surface flex h-full min-h-11 w-full min-w-[216px] items-center overflow-hidden rounded-[10px] bg-content/92 pr-1 text-content-fg"
      onPointerUpCapture={onPointerUp}
    >
      <div
        className="flex h-full grow cursor-grab items-center pr-1 pl-0.5 text-muted"
        data-tauri-drag-region
      >
        <GripVertical className="pointer-events-none" size={20} />
      </div>

      <div className="flex w-[68px] justify-center text-xs font-semibold tabular-nums">
        {isBusy ? (
          <ContentRotate className="text-muted" contentKey={status}>
            {status === "starting" ? "Starting" : "Finishing"}
          </ContentRotate>
        ) : (
          <div
            className={cn("flex transition-colors", isPaused && "text-muted")}
          >
            <RotatingDigits value={hours} />:
            <RotatingDigits value={minutes} />:
            <RotatingDigits value={seconds} />
          </div>
        )}
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
        {isBusy ? (
          <LoaderCircle className="animate-spin text-muted" size={ICON_SIZE} />
        ) : (
          <CircleStop
            className={cn(
              "transition-colors",
              isRecording && "animate-pulse text-error",
            )}
            size={ICON_SIZE}
          />
        )}
      </Button>

      <DiscardButton
        isDisabled={isBusy}
        key={sessionKey}
        onDiscard={onDiscard}
      />
    </main>
  );
}
