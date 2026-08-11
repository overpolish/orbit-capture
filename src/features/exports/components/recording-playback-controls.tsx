// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Pause, Play } from "lucide-react";

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
};

export function RecordingPlaybackControls({
  durationMs,
  isPlaying,
  onPause,
  onPlay,
  playhead,
}: RecordingPlaybackControlsProps) {
  return (
    <div className="flex h-7 shrink-0 items-center justify-center gap-1.5 border-t border-muted/15 px-3">
      <ToggleButton
        aria-keyshortcuts="Space"
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
    </div>
  );
}
