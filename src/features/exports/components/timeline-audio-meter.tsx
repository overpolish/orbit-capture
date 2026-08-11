// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useRef, useState } from "react";

import { AudioMeter } from "../../audio-inputs/components/audio-meter";
import { PreparedAudioTrack } from "../types";

import { AudioTrackVolumes, trackGain } from "./audio-level";
import { Playhead } from "./scrub-playhead";

const amplitudeToDecibels = (amplitude: number) =>
  Math.max(-60, 20 * Math.log10(Math.max(0.001, amplitude)));

export function TimelineAudioMeter({
  audioTracks,
  enabledTracks,
  height,
  playhead,
  volumes,
}: {
  audioTracks: PreparedAudioTrack[];
  enabledTracks: Set<number>;
  height: number;
  playhead: Playhead;
  volumes: AudioTrackVolumes;
}) {
  const [level, setLevel] = useState(-60);
  const [peak, setPeak] = useState(-60);
  const peakRef = useRef(-60);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        let amplitude = 0;
        for (const track of audioTracks) {
          if (!enabledTracks.has(track.streamIndex)) continue;
          const index = Math.min(
            track.waveform.length - 1,
            Math.max(0, Math.round(ratio * (track.waveform.length - 1))),
          );
          amplitude = Math.max(
            amplitude,
            (track.waveform[index] ?? 0) *
              trackGain(track.streamIndex, volumes),
          );
        }
        const next = amplitudeToDecibels(amplitude);
        peakRef.current = Math.max(next, peakRef.current - 0.7);
        setLevel(next);
        setPeak(peakRef.current);
      }),
    [audioTracks, enabledTracks, playhead, volumes],
  );

  return (
    <div className="shrink-0 pl-1">
      <AudioMeter
        compact
        decibels={level}
        height={height}
        hidePeakTick
        orientation="vertical"
        peak={peak}
        width={8}
      />
    </div>
  );
}
