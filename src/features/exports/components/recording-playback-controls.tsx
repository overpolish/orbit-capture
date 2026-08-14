// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Check, ClipboardCopy, Pause, Play } from "lucide-react";

import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { formatDuration } from "../duration";

import { Playhead } from "./scrub-playhead";
import { ElapsedTime } from "./scrub-timeline";

type RecordingPlaybackControlsProps = {
  durationMs: number;
  isPlaying: boolean;
  onPause: () => void;
  onPlay: () => void;
  playhead: Playhead;
  copyState?: "copying" | "done" | "idle";
  onCopyCurrentFrame?: () => void;
};

export function RecordingPlaybackControls({
  copyState = "idle",
  durationMs,
  isPlaying,
  onCopyCurrentFrame,
  onPause,
  onPlay,
  playhead,
}: RecordingPlaybackControlsProps) {
  return (
    <div className="relative flex h-7 shrink-0 items-center justify-center gap-1.5 border-t border-muted/15 px-3">
      <ToggleButton
        aria-keyshortcuts="P"
        aria-label={isPlaying ? "Pause preview" : "Play preview"}
        className="size-6 shrink-0"
        isSelected={isPlaying}
        off={<Play className="fill-current" size={14} />}
        onChange={(selected) => {
          if (selected) onPlay();
          else onPause();
        }}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <Pause className="fill-current" size={14} />
      </ToggleButton>
      <span className="min-w-24 text-xs font-light text-content-fg tabular-nums">
        <ElapsedTime playhead={playhead} />
        <span className="text-muted"> / {formatDuration(durationMs)}</span>
      </span>
      {onCopyCurrentFrame ? (
        <Button
          aria-label="Copy current frame"
          className="absolute right-3"
          isDisabled={copyState === "copying"}
          onPress={onCopyCurrentFrame}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          {copyState === "done" ? (
            <Check size={13} />
          ) : (
            <ClipboardCopy size={13} />
          )}
          {copyState === "copying"
            ? "Copying…"
            : copyState === "done"
              ? "Copied"
              : "Copy frame"}
        </Button>
      ) : null}
    </div>
  );
}
