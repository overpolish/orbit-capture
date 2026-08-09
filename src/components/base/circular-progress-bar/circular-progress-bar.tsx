// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { motion } from "motion/react";
import { ReactNode } from "react";
import {
  ProgressBar as AriaProgressBar,
  ProgressBarProps,
} from "react-aria-components";

import { tv } from "../../../lib/variants";

const ANIMATION_DURATION = 1.25;

const circularProgressBarVariants = tv({
  slots: {
    backdrop: "stroke-muted/15",
    base: "relative shrink-0",
    label:
      "absolute inset-0 flex items-center justify-center text-3xl font-bold text-content-fg tabular-nums",
    progress: "stroke-info [stroke-linecap:round]",
  },
});

type CircularProgressBarProps = Omit<
  ProgressBarProps,
  "children" | "className"
> & {
  hideBackdrop?: boolean;
  renderLabel?: (percentage?: number) => ReactNode;
  size?: number;
  strokeWidth?: number;
};

export function CircularProgressBar({
  hideBackdrop = false,
  isIndeterminate = false,
  renderLabel,
  size = 100,
  strokeWidth = 10,
  ...props
}: CircularProgressBarProps) {
  const { backdrop, base, label, progress } = circularProgressBarVariants();
  const radius = 50 - strokeWidth / 2;
  const circumference = 2 * Math.PI * radius;

  return (
    <AriaProgressBar
      className={base()}
      isIndeterminate={isIndeterminate}
      style={{ height: size, width: size }}
      {...props}
    >
      {({ percentage }) => (
        <>
          <svg
            aria-hidden="true"
            className="size-full fill-none"
            strokeWidth={strokeWidth}
            viewBox="0 0 100 100"
          >
            {!hideBackdrop ? (
              <circle className={backdrop()} cx="50" cy="50" r={radius} />
            ) : null}

            {isIndeterminate ? (
              <motion.circle
                animate={{
                  rotate: [0, 180, 360],
                  strokeDasharray: [
                    `${(circumference * 0.1).toString()} ${circumference.toString()}`,
                    `${(circumference * 0.25).toString()} ${circumference.toString()}`,
                    `${(circumference * 0.1).toString()} ${circumference.toString()}`,
                  ],
                  strokeDashoffset: [
                    circumference * 0.45,
                    circumference * 0.67,
                    circumference * 0.45,
                  ],
                }}
                className={progress()}
                cx="50"
                cy="50"
                r={radius}
                strokeWidth={strokeWidth}
                style={{ transformOrigin: "50% 50%" }}
                transition={{
                  rotate: {
                    duration: ANIMATION_DURATION,
                    ease: "linear",
                    repeat: Infinity,
                  },
                  strokeDasharray: {
                    duration: ANIMATION_DURATION,
                    ease: "easeInOut",
                    repeat: Infinity,
                  },
                  strokeDashoffset: {
                    duration: ANIMATION_DURATION,
                    ease: "easeInOut",
                    repeat: Infinity,
                  },
                }}
              />
            ) : null}

            {percentage !== undefined && !isIndeterminate ? (
              <motion.circle
                animate={{
                  strokeDashoffset: circumference * (1 - percentage / 100),
                }}
                className={progress()}
                cx="50"
                cy="50"
                initial={false}
                r={radius}
                strokeDasharray={circumference}
                strokeWidth={strokeWidth}
                transform="rotate(-90 50 50)"
                transition={{ duration: 0.5, ease: "easeInOut" }}
              />
            ) : null}
          </svg>

          {renderLabel?.(percentage) ??
            (percentage !== undefined && !isIndeterminate ? (
              <span className={label()}>{percentage.toFixed(0)}</span>
            ) : null)}
        </>
      )}
    </AriaProgressBar>
  );
}
