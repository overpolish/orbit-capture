// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ClipboardCopy, Pause, Play } from "lucide-react";
import { memo } from "react";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { CheckOnClickButton } from "../../../components/shared/check-on-click-button/check-on-click-button";
import { formatDuration } from "../duration";

import { Playhead } from "./scrub-playhead";
import { ElapsedTime } from "./scrub-timeline";

type RecordingPlaybackControlsProps = {
  durationMs: number;
  isPlaying: boolean;
  onPause: () => void;
  onPlay: () => void;
  playhead: Playhead;
  // Returning a promise makes the copy button await the copy before it checks.
  onCopyCurrentFrame?: () => Promise<unknown> | undefined;
};

/**
 * Memoized: the playhead publishes its own time through a subscription, so
 * nothing here changes while an output draft updates at pointer rate.
 */
export const RecordingPlaybackControls = memo(
  function RecordingPlaybackControls({
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
          <CheckOnClickButton
            aria-label="Copy current frame"
            className="absolute right-3"
            color="muted"
            onPress={() => onCopyCurrentFrame()}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <ClipboardCopy size={13} />
            Copy frame
          </CheckOnClickButton>
        ) : null}
      </div>
    );
  },
);
