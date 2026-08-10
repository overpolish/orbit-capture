// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Pause, Play } from "lucide-react";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { formatDuration } from "../duration";

import { Playhead } from "./scrub-playhead";
import { ElapsedTime, SeekHandler, Timeline } from "./scrub-timeline";

type RecordingPlaybackControlsProps = {
  durationMs: number;
  isPlaying: boolean;
  onPause: () => void;
  onPlay: () => void;
  onSeek: SeekHandler;
  playhead: Playhead;
};

export function RecordingPlaybackControls({
  durationMs,
  isPlaying,
  onPause,
  onPlay,
  onSeek,
  playhead,
}: RecordingPlaybackControlsProps) {
  return (
    <div className="flex items-center gap-2">
      <div className="flex w-36 shrink-0 items-center gap-2">
        <ToggleButton
          aria-label={isPlaying ? "Pause preview" : "Play preview"}
          className="size-6 shrink-0"
          isSelected={isPlaying}
          off={<Play className="fill-current" size={16} />}
          onChange={(selected) => {
            if (selected) onPlay();
            else onPause();
          }}
          showFocus={false}
          size="sm"
          variant="ghost"
        >
          <Pause className="fill-current" size={16} />
        </ToggleButton>
        <span className="min-w-0 text-xxs text-muted tabular-nums">
          <ElapsedTime playhead={playhead} /> / {formatDuration(durationMs)}
        </span>
      </div>
      <Timeline onSeek={onSeek} playhead={playhead} />
    </div>
  );
}
