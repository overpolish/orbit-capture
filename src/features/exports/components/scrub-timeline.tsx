// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  PointerEvent as ReactPointerEvent,
  useEffect,
  useMemo,
  useRef,
} from "react";

import { formatDuration } from "../duration";
import { PreparedAudioTrack } from "../types";

import { clamp, Playhead } from "./scrub-playhead";

const waveformPath = (points: number[]) => {
  if (points.length === 0) return "";
  const center = 20;
  const scale = 17;
  return points
    .map((peak, index) => {
      const x = (index / Math.max(1, points.length - 1)) * 1000;
      const height = Math.max(0.75, peak * scale);
      return `M${x.toFixed(2)} ${(center - height).toFixed(2)}V${(center + height).toFixed(2)}`;
    })
    .join(" ");
};

export function Waveform({
  enabled,
  onSeek,
  playhead,
  track,
}: {
  enabled: boolean;
  onSeek: (ratio: number) => void;
  playhead: Playhead;
  track: PreparedAudioTrack;
}) {
  const path = useMemo(() => waveformPath(track.waveform), [track.waveform]);
  const lineRef = useRef<HTMLDivElement>(null);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        if (lineRef.current)
          lineRef.current.style.left = `${(ratio * 100).toString()}%`;
      }),
    [playhead],
  );

  const seek = (event: ReactPointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1));
  };

  return (
    <div
      className="relative h-6 min-w-0 grow cursor-ew-resize overflow-hidden rounded bg-muted/8"
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        seek(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) seek(event);
      }}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
    >
      <svg
        aria-hidden="true"
        className={enabled ? "size-full text-info" : "size-full text-muted/35"}
        preserveAspectRatio="none"
        viewBox="0 0 1000 40"
      >
        <path
          className="stroke-current"
          d={path}
          fill="none"
          strokeWidth="2"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      <div
        className="pointer-events-none absolute inset-y-0 w-px bg-content-fg/80"
        ref={lineRef}
        style={{ left: "0%" }}
      />
    </div>
  );
}

export function Timeline({
  onSeek,
  playhead,
}: {
  onSeek: (ratio: number) => void;
  playhead: Playhead;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const fillRef = useRef<HTMLDivElement>(null);
  const knobRef = useRef<HTMLDivElement>(null);
  const ratioRef = useRef(0);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        ratioRef.current = ratio;
        const percent = `${(ratio * 100).toString()}%`;
        if (fillRef.current) fillRef.current.style.width = percent;
        if (knobRef.current) knobRef.current.style.left = percent;
        // Assistive technology needs the position too, and this element is
        // never re-rendered, so React will not overwrite the attribute.
        rootRef.current?.setAttribute(
          "aria-valuenow",
          Math.round(ratio * 100).toString(),
        );
      }),
    [playhead],
  );

  const seek = (event: ReactPointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1));
  };

  return (
    <div
      aria-label="Recording position"
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={0}
      className="relative h-6 min-w-0 grow cursor-ew-resize touch-none"
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        onSeek(
          clamp(
            ratioRef.current + (event.key === "ArrowRight" ? 0.01 : -0.01),
            0,
            1,
          ),
        );
      }}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        seek(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) seek(event);
      }}
      onPointerUp={(event) => {
        event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      ref={rootRef}
      role="slider"
      tabIndex={0}
    >
      <div className="absolute inset-x-0 top-1/2 h-1.5 -translate-y-1/2 overflow-hidden rounded-full bg-muted/15">
        <div
          className="h-full rounded-full bg-info"
          ref={fillRef}
          style={{ width: "0%" }}
        />
      </div>
      <div
        className="pointer-events-none absolute top-1/2 size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-content bg-info shadow-sm"
        ref={knobRef}
        style={{ left: "0%" }}
      />
    </div>
  );
}

/** The elapsed half of the time readout, written straight to the text node. */
export function ElapsedTime({ playhead }: { playhead: Playhead }) {
  const ref = useRef<HTMLSpanElement>(null);

  useEffect(
    () =>
      playhead.subscribe((seconds) => {
        const text = formatDuration(seconds * 1000);
        if (ref.current && ref.current.textContent !== text)
          ref.current.textContent = text;
      }),
    [playhead],
  );

  return <span ref={ref}>{formatDuration(0)}</span>;
}
