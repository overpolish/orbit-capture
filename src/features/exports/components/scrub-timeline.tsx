// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  PointerEvent as ReactPointerEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { formatDuration } from "../duration";
import { PreparedAudioTrack } from "../types";

import { decibelGain } from "./audio-level";
import { clamp, Playhead } from "./scrub-playhead";

export type ScrubPhase = "end" | "move" | "start";
export type SeekHandler = (ratio: number, phase: ScrubPhase) => void;

const TICK_INTERVALS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
const MINIMUM_TICK_SPACING = 70;

const waveformPath = (points: number[], volumeDecibels: number) => {
  if (points.length === 0) return "";
  const center = 20;
  return points
    .map((peak, index) => {
      const x = (index / Math.max(1, points.length - 1)) * 1000;
      const adjustedPeak = Math.min(1, peak * decibelGain(volumeDecibels));
      const height = Math.max(
        1.25,
        Math.pow(Math.max(0, adjustedPeak), 0.55) * 18.5,
      );
      return `M${x.toFixed(2)} ${(center - height).toFixed(2)}V${(center + height).toFixed(2)}`;
    })
    .join(" ");
};

export function Waveform({
  enabled,
  onSelect,
  track,
  volumeDecibels,
}: {
  enabled: boolean;
  onSelect: () => void;
  track: PreparedAudioTrack;
  volumeDecibels: number;
}) {
  const path = useMemo(
    () => waveformPath(track.waveform, volumeDecibels),
    [track.waveform, volumeDecibels],
  );

  return (
    <div
      className="relative h-8 min-w-0 grow cursor-default overflow-hidden rounded bg-muted/8"
      data-audio-stream-index={track.streamIndex}
      onClick={onSelect}
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
    </div>
  );
}

export function TimelineScrubber({
  onSeek,
  playhead,
}: {
  onSeek: SeekHandler;
  playhead: Playhead;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const lineRef = useRef<HTMLDivElement>(null);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        if (lineRef.current)
          lineRef.current.style.left = `${(ratio * 100).toString()}%`;
      }),
    [playhead],
  );

  const seek = (
    event: ReactPointerEvent<HTMLDivElement>,
    phase: ScrubPhase,
  ) => {
    const bounds = rootRef.current?.getBoundingClientRect();
    if (!bounds) return;
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1), phase);
  };

  return (
    <div
      className="pointer-events-none absolute inset-0 overflow-hidden"
      ref={rootRef}
    >
      <div
        className="pointer-events-auto absolute inset-y-0 w-3 -translate-x-1/2 cursor-ew-resize touch-none"
        onPointerCancel={(event) => {
          seek(event, "end");
          if (event.currentTarget.hasPointerCapture(event.pointerId))
            event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onPointerDown={(event) => {
          event.preventDefault();
          event.currentTarget.setPointerCapture(event.pointerId);
          seek(event, "start");
        }}
        onPointerMove={(event) => {
          if (event.currentTarget.hasPointerCapture(event.pointerId))
            seek(event, "move");
        }}
        onPointerUp={(event) => {
          seek(event, "end");
          event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        ref={lineRef}
        style={{ left: "0%" }}
      >
        <span className="pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-content-fg/80" />
      </div>
    </div>
  );
}

export function TimelineRuler({
  durationMs,
  onSeek,
  playhead,
}: {
  durationMs: number;
  onSeek: SeekHandler;
  playhead: Playhead;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const ratioRef = useRef(0);
  const [width, setWidth] = useState(0);
  const durationSeconds = Math.max(0, durationMs / 1_000);
  const pixelsPerSecond = width / Math.max(1, durationSeconds);
  const interval =
    TICK_INTERVALS.find(
      (candidate) => candidate * pixelsPerSecond >= MINIMUM_TICK_SPACING,
    ) ?? TICK_INTERVALS[TICK_INTERVALS.length - 1];
  const ticks = useMemo(() => {
    if (durationSeconds <= 0) return [0];
    return Array.from(
      { length: Math.floor(durationSeconds / interval) + 1 },
      (_, index) => index * interval,
    );
  }, [durationSeconds, interval]);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const observer = new ResizeObserver(() => {
      setWidth(root.clientWidth);
    });
    observer.observe(root);
    setWidth(root.clientWidth);
    return () => {
      observer.disconnect();
    };
  }, []);

  useEffect(
    () =>
      playhead.subscribe((_seconds, ratio) => {
        ratioRef.current = ratio;
        // Assistive technology needs the position too, and this element is
        // never re-rendered, so React will not overwrite the attribute.
        rootRef.current?.setAttribute(
          "aria-valuenow",
          Math.round(ratio * 100).toString(),
        );
      }),
    [playhead],
  );

  const seek = (
    event: ReactPointerEvent<HTMLDivElement>,
    phase: ScrubPhase,
  ) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    onSeek(clamp((event.clientX - bounds.left) / bounds.width, 0, 1), phase);
  };

  return (
    <div
      aria-label="Recording position"
      aria-valuemax={100}
      aria-valuemin={0}
      aria-valuenow={0}
      className="relative h-4 min-w-0 grow cursor-ew-resize touch-none overflow-hidden outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-content-fg/75"
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        const ratio = clamp(
          ratioRef.current + (event.key === "ArrowRight" ? 0.01 : -0.01),
          0,
          1,
        );
        onSeek(ratio, "start");
        onSeek(ratio, "end");
      }}
      onPointerCancel={(event) => {
        seek(event, "end");
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          event.currentTarget.releasePointerCapture(event.pointerId);
        event.currentTarget.blur();
      }}
      onPointerDown={(event) => {
        // Pointer scrubbing must not leave the ruler as the keyboard target.
        // Otherwise the next Space press exposes WebKit's focus treatment
        // instead of reaching the export-window playback shortcut.
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        seek(event, "start");
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          seek(event, "move");
      }}
      onPointerUp={(event) => {
        seek(event, "end");
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          event.currentTarget.releasePointerCapture(event.pointerId);
        event.currentTarget.blur();
      }}
      ref={rootRef}
      role="slider"
      tabIndex={0}
    >
      {ticks.map((seconds) => {
        const label = formatDuration(seconds * 1_000);
        const x = (seconds / Math.max(1, durationSeconds)) * width;
        // Match the native timeline: labels always sit after their tick and
        // disappear when they would not fit, rather than flipping to the
        // other side at the trailing edge.
        const showLabel = width - x >= label.length * 6 + 4;
        return (
          <div
            className="pointer-events-none absolute inset-y-0 border-l border-muted/35"
            key={seconds}
            style={{
              left: `${((seconds / Math.max(1, durationSeconds)) * 100).toString()}%`,
            }}
          >
            {showLabel ? (
              <span className="absolute left-1 top-0 whitespace-nowrap text-xxs font-medium text-muted tabular-nums">
                {label}
              </span>
            ) : null}
          </div>
        );
      })}
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
