// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Mic, Volume2 } from "lucide-react";

import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { PreparedAudioTrack } from "../types";

import { AudioTrackVolumes } from "./audio-level";
import { Waveform } from "./scrub-timeline";

export function ScrubAudioTracks({
  audioTracks,
  enabledTracks,
  onEnabledTracksChange,
  onSelectTrack,
  selectedTrack,
  volumes,
}: {
  audioTracks: PreparedAudioTrack[];
  enabledTracks: Set<number>;
  onEnabledTracksChange: (tracks: Set<number>) => void;
  onSelectTrack: (streamIndex: number) => void;
  selectedTrack: number | null;
  volumes: AudioTrackVolumes;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      {audioTracks.map((track) => {
        const enabled = enabledTracks.has(track.streamIndex);
        const Icon = track.kind === "microphone" ? Mic : Volume2;
        return (
          <div className="flex items-center gap-2" key={track.streamIndex}>
            <div
              className={`flex h-8 w-36 shrink-0 items-center gap-2 rounded px-2 text-xs font-medium text-content-fg transition-colors ${selectedTrack === track.streamIndex ? "bg-info/15" : ""}`}
              onClick={() => {
                onSelectTrack(track.streamIndex);
              }}
            >
              <Checkbox
                aria-label={`${enabled ? "Exclude" : "Include"} ${track.label}`}
                isSelected={enabled}
                onChange={() => {
                  const next = new Set(enabledTracks);
                  if (next.has(track.streamIndex))
                    next.delete(track.streamIndex);
                  else next.add(track.streamIndex);
                  onEnabledTracksChange(next);
                }}
                size="xs"
              />
              <Icon className="shrink-0 text-muted" size={14} />
              <span className="min-w-0 grow truncate">{track.label}</span>
            </div>
            <Waveform
              enabled={enabled}
              onSelect={() => {
                onSelectTrack(track.streamIndex);
              }}
              track={track}
              volumeDecibels={volumes.get(track.streamIndex) ?? 0}
            />
          </div>
        );
      })}
    </div>
  );
}
