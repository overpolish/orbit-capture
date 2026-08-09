// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Check, Mic, Volume2 } from "lucide-react";

import { Button } from "../../../components/base/button/button";
import { PreparedAudioTrack } from "../types";

import { Playhead } from "./scrub-playhead";
import { Waveform } from "./scrub-timeline";

export function ScrubAudioTracks({
  audioTracks,
  enabledTracks,
  onEnabledTracksChange,
  onSeek,
  playhead,
}: {
  audioTracks: PreparedAudioTrack[];
  enabledTracks: Set<number>;
  onEnabledTracksChange: (tracks: Set<number>) => void;
  onSeek: (ratio: number) => void;
  playhead: Playhead;
}) {
  return (
    <div className="flex flex-col gap-2">
      {audioTracks.map((track) => {
        const enabled = enabledTracks.has(track.streamIndex);
        return (
          <div className="flex items-center gap-2" key={track.streamIndex}>
            <Button
              aria-label={`${enabled ? "Exclude" : "Include"} ${track.label}`}
              className="group w-36 justify-start"
              onPress={() => {
                const next = new Set(enabledTracks);
                if (next.has(track.streamIndex)) next.delete(track.streamIndex);
                else next.add(track.streamIndex);
                onEnabledTracksChange(next);
              }}
              showFocus={false}
              size="sm"
              variant={enabled ? "soft" : "ghost"}
            >
              {track.kind === "microphone" ? (
                <Mic size={15} />
              ) : (
                <Volume2 size={15} />
              )}
              <span className="min-w-0 grow truncate text-left">
                {track.label}
              </span>
              {enabled ? <Check className="text-success" size={14} /> : null}
            </Button>
            <Waveform
              enabled={enabled}
              onSeek={onSeek}
              playhead={playhead}
              track={track}
            />
          </div>
        );
      })}
    </div>
  );
}
