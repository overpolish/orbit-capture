// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { SVGAttributes, useEffect, useRef, useState } from "react";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const decibelToPercentage = (decibel: number): number => {
  if (decibel < -60) return 0;
  if (decibel > 0) return 100;

  const normalized = (decibel + 60) / 60;
  const power = 1.357; // -24 dB map to ~50%
  return Math.pow(normalized, power) * 100;
};

let nextMeterId = 0;

const ticksForWidth = (width: number) => {
  const ticks = [-48, -24];
  if (width > 70) ticks.push(-12);
  if (width > 5) ticks.push(-3);
  return ticks;
};

const tickVariants = tv({
  defaultVariants: {
    position: "below",
  },
  slots: {
    base: "absolute flex flex-col -translate-x-[50%] text-muted items-center pointer-events-none select-none",
    label: "relative text-[6px]/2 text-shadow-2xs transition-colors px-0.25",
    line: "w-[1px] h-[2px] bg-muted transition-colors",
  },
  variants: {
    clipping: {
      true: {
        label: "text-warning-100",
        line: "bg-warning-100",
      },
    },
    position: {
      above: { base: "flex-col-reverse", label: "mb-[1px]" },
      below: { base: "flex-col mt-[1.5px]" },
    },
  },
});

type TickProps = VariantProps<typeof tickVariants> & {
  tick: number;
  display?: string;
  excludeLine?: boolean;
  labelClassName?: string;
  maxTick?: number;
};
const Tick = ({
  display,
  excludeLine = false,
  labelClassName,
  maxTick,
  position,
  tick,
}: TickProps) => {
  const { base, label, line } = tickVariants({ clipping: tick > 0, position });
  return (
    <div
      className={base()}
      key={tick}
      style={{
        left:
          decibelToPercentage(Math.min(maxTick ?? Infinity, tick)).toString() +
          "%",
      }}
    >
      {!excludeLine && <div className={line()} />}
      <span className={label({ className: labelClassName })}>
        {display ?? tick}
      </span>
    </div>
  );
};

type AudioMeterProps = {
  decibels: number;
  disabled?: boolean;
  height?: number;
  hidePeakTick?: boolean;
  hideTicks?: boolean;
  peak?: number;
  radius?: number;
  width?: number | string;
};

export const AudioMeter = ({
  decibels,
  disabled,
  height = 10,
  hidePeakTick,
  hideTicks,
  peak = -Infinity,
  radius = 2,
  width = 150,
}: AudioMeterProps) => {
  const idRef = useRef<number | null>(null);
  idRef.current ??= nextMeterId++;
  const id = idRef.current;
  const fillId = `meter-fill-${id.toString()}`;
  const meterClipId = `meter-clip-${id.toString()}`;
  const peakClipId = `peak-clip-${id.toString()}`;
  const percentage = decibelToPercentage(decibels);
  const peakPercentage = decibelToPercentage(Math.min(peak, -0.5));

  const svgRef = useRef<SVGSVGElement>(null);
  const [ticks, setTicks] = useState(() =>
    ticksForWidth(typeof width === "number" ? width : 0),
  );

  const METER: SVGAttributes<SVGRectElement> = {
    height: "100%",
    rx: radius,
    ry: radius,
    width: disabled ? "0%" : "100%",
  };

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;

    const resizeObserver = new ResizeObserver(([entry]) => {
      setTicks(ticksForWidth(entry.contentRect.width));
    });
    resizeObserver.observe(svg);

    return () => {
      resizeObserver.disconnect();
    };
  }, []);

  return (
    <div className="pointer-events-none select-none">
      {/* Using SVG due to layering divs with border-radius and linear gradient
       * causing bleeding */}
      <svg
        height={height}
        ref={svgRef}
        viewBox={`0 0 ${width.toString()} ${height.toString()}`}
        width={width}
      >
        <defs>
          <linearGradient id={fillId} x1="0%" x2="100%" y1="0%" y2="0%">
            <stop offset="0%" stopColor="var(--color-success)" />
            <stop offset="65%" stopColor="var(--color-success)" />
            <stop offset="85%" stopColor="var(--color-warning)" />
            <stop offset="93%" stopColor="var(--color-warning)" />
            <stop offset="96%" stopColor="var(--color-warning-100)" />
            <stop offset="100%" stopColor="var(--color-warning-100)" />{" "}
          </linearGradient>

          <clipPath id={meterClipId}>
            <rect height="100%" width={percentage.toString() + "%"} />
          </clipPath>

          <clipPath id={peakClipId}>
            {peak >= -60 && (
              <rect
                height="100%"
                transform="translate(-1.5,0)"
                width="2px"
                x={peakPercentage.toString() + "%"}
              />
            )}
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
        <div className="relative h-3">
          {!hideTicks &&
            [...ticks].map((tick) => <Tick key={tick} tick={tick} />)}

          {!hidePeakTick && !disabled && peak >= -60 && (
            <Tick
              display={peak.toFixed(1)}
              labelClassName="backdrop-blur-xs bg-content/50"
              maxTick={-0.5}
              position="below"
              tick={peak}
            />
          )}
        </div>
      )}
    </div>
  );
};
