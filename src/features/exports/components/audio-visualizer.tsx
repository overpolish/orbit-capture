// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useRef } from "react";

import { PreparedAudioTrack } from "../types";

import { AudioTrackVolumes, trackGain } from "./audio-level";
import { Playhead } from "./scrub-playhead";

const BAR_COUNT = 48;
const BAND_COUNT = BAR_COUNT / 2;

export function AudioVisualizer({
  audioTracks,
  enabledTracks,
  playhead,
  volumes,
}: {
  audioTracks: PreparedAudioTrack[];
  enabledTracks: Set<number>;
  playhead: Playhead;
  volumes: AudioTrackVolumes;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const playheadRatio = { current: 0 };
    const size = { height: 1, scale: 1, width: 1 };

    const resize = () => {
      const bounds = canvas.getBoundingClientRect();
      const scale = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(bounds.width * scale));
      const height = Math.max(1, Math.round(bounds.height * scale));
      canvas.width = width;
      canvas.height = height;
      size.height = height;
      size.scale = scale;
      size.width = width;
    };

    const tracks = audioTracks.filter((track) =>
      enabledTracks.has(track.streamIndex),
    );
    const draw = () => {
      const context = canvas.getContext("2d");
      if (!context) return;
      const { height, scale, width } = size;
      context.clearRect(0, 0, width, height);
      const gap = 5 * scale;
      const barWidth = Math.max(
        scale,
        (width - gap * (BAR_COUNT - 1)) / BAR_COUNT,
      );
      const center = height / 2;
      const maximum = height * 0.36;
      context.fillStyle = getComputedStyle(canvas).color;

      for (let band = 0; band < BAND_COUNT; band += 1) {
        const distance = band / BAND_COUNT;
        let amplitude = 0;
        for (const track of tracks) {
          const sampleOffset = Math.round((band - BAND_COUNT / 2) * 0.35);
          const index = Math.min(
            track.waveform.length - 1,
            Math.max(
              0,
              Math.round(playheadRatio.current * (track.waveform.length - 1)) +
                sampleOffset,
            ),
          );
          amplitude = Math.max(
            amplitude,
            (track.waveform[index] ?? 0) *
              trackGain(track.streamIndex, volumes),
          );
        }
        const shape = 0.45 + Math.sin((1 - distance) * Math.PI) * 0.55;
        const heightForBand = Math.max(
          2 * scale,
          Math.sqrt(amplitude) * maximum * shape,
        );
        for (const bar of [band, BAR_COUNT - 1 - band]) {
          const x = bar * (barWidth + gap);
          context.globalAlpha = 0.45 + (band / BAND_COUNT) * 0.55;
          context.beginPath();
          context.roundRect(
            x,
            center - heightForBand,
            barWidth,
            heightForBand * 2,
            barWidth / 2,
          );
          context.fill();
        }
      }
      context.globalAlpha = 1;
    };

    const resizeObserver = new ResizeObserver(() => {
      resize();
      draw();
    });
    resizeObserver.observe(canvas);
    const unsubscribe = playhead.subscribe((_seconds, ratio) => {
      playheadRatio.current = ratio;
      draw();
    });
    resize();
    draw();
    return () => {
      resizeObserver.disconnect();
      unsubscribe();
    };
  }, [audioTracks, enabledTracks, playhead, volumes]);

  return (
    <div className="flex min-h-0 grow items-center justify-center overflow-hidden px-[12%]">
      <canvas
        aria-label="Audio visualizer"
        className="h-48 max-h-[55%] w-full text-info"
        ref={canvasRef}
        role="img"
      />
    </div>
  );
}
