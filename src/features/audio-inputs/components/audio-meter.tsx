// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { SVGAttributes, useEffect, useRef, useState } from "react";

const decibelToPercentage = (decibel: number): number => {
  if (decibel < -60) return 0;
  if (decibel > 0) return 100;

  const normalized = (decibel + 60) / 60;
  const power = 1.357; // -24 dB map to ~50%
  return Math.pow(normalized, power) * 100;
};

let nextMeterId = 0;

const ticksForLength = (length: number) => {
  const ticks = [-48, -24];
  if (length > 70) ticks.push(-12);
  if (length > 95) ticks.push(-3);
  return ticks;
};

type TickProps = {
  tick: number;
  display?: string;
  excludeLine?: boolean;
  labelClassName?: string;
  maxTick?: number;
  orientation?: "horizontal" | "vertical";
  position?: "above" | "below";
};
const Tick = ({
  display,
  excludeLine = false,
  labelClassName,
  maxTick,
  orientation = "horizontal",
  position = "below",
  tick,
}: TickProps) => {
  const percentage = decibelToPercentage(Math.min(maxTick ?? Infinity, tick));
  const clipping = tick > 0;
  const vertical = orientation === "vertical";
  return (
    <div
      className={`pointer-events-none absolute flex items-center text-muted select-none ${
        vertical
          ? "-translate-y-1/2 flex-row"
          : position === "above"
            ? "-translate-x-1/2 flex-col-reverse"
            : "mt-[1.5px] -translate-x-1/2 flex-col"
      }`}
      key={tick}
      style={
        vertical
          ? { bottom: `${percentage.toString()}%` }
          : { left: `${percentage.toString()}%` }
      }
    >
      {!excludeLine && (
        <div
          className={`${vertical ? "h-px w-0.5" : "h-0.5 w-px"} ${
            clipping ? "bg-warning-100" : "bg-muted"
          } transition-colors`}
        />
      )}
      <span
        className={`relative px-0.25 text-[6px]/2 text-shadow-2xs transition-colors ${
          vertical ? "ml-px" : position === "above" ? "mb-px" : ""
        } ${clipping ? "text-warning-100" : ""} ${labelClassName ?? ""}`}
      >
        {display ?? tick}
      </span>
    </div>
  );
};

type AudioMeterProps = {
  decibels: number;
  compact?: boolean;
  disabled?: boolean;
  height?: number;
  hidePeakTick?: boolean;
  hideTicks?: boolean;
  orientation?: "horizontal" | "vertical";
  peak?: number;
  radius?: number;
  width?: number | string;
};

export const AudioMeter = ({
  compact = false,
  decibels,
  disabled,
  height,
  hidePeakTick,
  hideTicks,
  orientation = "horizontal",
  peak = -Infinity,
  radius = 2,
  width,
}: AudioMeterProps) => {
  const vertical = orientation === "vertical";
  const meterHeight = height ?? (vertical ? 150 : 10);
  const meterWidth = width ?? (vertical ? 10 : 150);
  const idRef = useRef<number | null>(null);
  idRef.current ??= nextMeterId++;
  const id = idRef.current;
  const fillId = `meter-fill-${id.toString()}`;
  const meterClipId = `meter-clip-${id.toString()}`;
  const peakClipId = `peak-clip-${id.toString()}`;
  const percentage = disabled ? 0 : decibelToPercentage(decibels);
  const peakPercentage = decibelToPercentage(Math.min(peak, -0.5));

  const svgRef = useRef<SVGSVGElement>(null);
  const [ticks, setTicks] = useState(() =>
    ticksForLength(
      vertical ? meterHeight : typeof meterWidth === "number" ? meterWidth : 0,
    ),
  );

  const METER: SVGAttributes<SVGRectElement> = {
    height: "100%",
    rx: radius,
    ry: radius,
    width: "100%",
  };

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;

    const resizeObserver = new ResizeObserver(([entry]) => {
      setTicks(
        ticksForLength(
          vertical ? entry.contentRect.height : entry.contentRect.width,
        ),
      );
    });
    resizeObserver.observe(svg);

    return () => {
      resizeObserver.disconnect();
    };
  }, [vertical]);

  return (
    <div
      className={`pointer-events-none select-none ${vertical ? "flex items-stretch" : ""}`}
    >
      {/* Using SVG due to layering divs with border-radius and linear gradient
       * causing bleeding */}
      <svg
        height={meterHeight}
        preserveAspectRatio="none"
        ref={svgRef}
        viewBox={`0 0 ${typeof meterWidth === "number" ? meterWidth.toString() : "150"} ${meterHeight.toString()}`}
        width={meterWidth}
      >
        <defs>
          <linearGradient
            id={fillId}
            x1="0%"
            x2={vertical ? "0%" : "100%"}
            y1={vertical ? "100%" : "0%"}
            y2="0%"
          >
            <stop offset="0%" stopColor="var(--color-success)" />
            <stop offset="65%" stopColor="var(--color-success)" />
            <stop offset="85%" stopColor="var(--color-warning)" />
            <stop offset="93%" stopColor="var(--color-warning)" />
            <stop offset="96%" stopColor="var(--color-warning-100)" />
            <stop offset="100%" stopColor="var(--color-warning-100)" />
          </linearGradient>

          <clipPath id={meterClipId}>
            {vertical ? (
              <rect
                height={`${percentage.toString()}%`}
                width="100%"
                y={`${(100 - percentage).toString()}%`}
              />
            ) : (
              <rect height="100%" width={`${percentage.toString()}%`} />
            )}
          </clipPath>

          <clipPath id={peakClipId}>
            {!disabled &&
              peak >= -60 &&
              (vertical ? (
                <rect
                  height="2px"
                  transform="translate(0,-1)"
                  width="100%"
                  y={`${(100 - peakPercentage).toString()}%`}
                />
              ) : (
                <rect
                  height="100%"
                  transform="translate(-1.5,0)"
                  width="2px"
                  x={`${peakPercentage.toString()}%`}
                />
              ))}
          </clipPath>
        </defs>

        <rect className="fill-muted/20" {...METER} width="100%" />
        <rect
          clipPath={`url(#${meterClipId})`}
          fill={`url(#${fillId})`}
          {...METER}
        />
        <rect
          clipPath={`url(#${peakClipId})`}
          fill={`url(#${fillId})`}
          {...METER}
        />
      </svg>

      {(!hideTicks || !hidePeakTick) && (
        <div
          className={
            vertical ? `relative ${compact ? "w-3" : "w-7"}` : "relative h-3"
          }
        >
          {!hideTicks &&
            [...ticks].map((tick) => (
              <Tick
                key={tick}
                labelClassName={compact ? "text-[5px]" : undefined}
                orientation={orientation}
                tick={tick}
              />
            ))}

          {!hidePeakTick && !disabled && peak >= -60 && (
            <Tick
              display={peak.toFixed(1)}
              labelClassName="backdrop-blur-xs bg-content/50"
              maxTick={-0.5}
              orientation={orientation}
              position="below"
              tick={peak}
            />
          )}
        </div>
      )}
    </div>
  );
};
