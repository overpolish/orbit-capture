// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { LucideIcon } from "lucide-react";
import { motion, MotionProps } from "motion/react";
import { createElement, useState } from "react";

import { randomInRange, Range } from "../../../lib/math";
import { tv } from "../../../lib/variants";

const sparkleVariants = tv({
  base: "pointer-events-none absolute z-20",
  variants: {
    default: {
      true: "text-yellow-500",
    },
  },
});

const SPARKLE_KEYS = Array.from({ length: 256 }, () => crypto.randomUUID());

type Sparkle = {
  delay: number;
  duration: number;
  scale: number;
  x: string;
  y: string;
  color?: string;
};

type GenerateSparkle = {
  duration: Range;
  scale: Range;
  colors?: string[];
  offset?: { x?: Range; y?: Range };
};
const generateSparkle = ({
  colors,
  duration,
  offset,
  scale,
}: GenerateSparkle): Sparkle => {
  const x = randomInRange(offset?.x ?? { max: 100, min: -10 }).toString() + "%";
  const y = randomInRange(offset?.y ?? { max: 75, min: -10 }).toString() + "%";
  return {
    color:
      colors && colors.length > 0
        ? colors[Math.floor(Math.random() * colors.length)]
        : undefined,
    delay: randomInRange({ max: 1.5, min: 0.1 }),
    duration: randomInRange(duration),
    scale: randomInRange(scale),
    x,
    y,
  };
};

type SparkleProps = Required<Omit<SparklesProps, "children" | "sparklesCount">>;
type SparkleAnimation = {
  key: string;
  props: MotionProps;
};

const Sparkle = ({
  colors,
  duration,
  fillType,
  icon,
  offset,
  opacity,
  rotate,
  scale,
}: SparkleProps) => {
  const createAnimation = (): SparkleAnimation => {
    const newSparkle = generateSparkle({ colors, duration, offset, scale });
    return {
      key: crypto.randomUUID(),
      props: {
        animate: {
          opacity: [0, opacity, 0],
          rotate,
          scale: [0, newSparkle.scale, 0],
        },
        initial: {
          color: newSparkle.color,
          left: newSparkle.x,
          top: newSparkle.y,
        },
        transition: {
          delay: newSparkle.delay,
          duration: newSparkle.duration,
        },
      },
    };
  };

  const [animation, setAnimation] = useState<SparkleAnimation>(createAnimation);

  const startAnimation = () => {
    setAnimation(createAnimation());
  };

  return (
    <motion.span
      key={animation.key}
      {...animation.props}
      className={sparkleVariants({ default: colors.length === 0 })}
      onAnimationComplete={startAnimation}
    >
      {createElement(icon, {
        className:
          fillType === "fill-only"
            ? "fill-current stroke-transparent"
            : fillType === "stroke-only"
              ? "fill-transparent stroke-current"
              : "fill-current stroke-current",
      })}
    </motion.span>
  );
};

type SparklesProps = {
  icon: LucideIcon;
  children?: React.ReactNode;
  /**
   * @default undefined
   * @type string[]
   * @description
   * SVG fill compatible colors
   */
  colors?: string[];
  /**
   * @default { min: 0.4, max: 1.2 }
   * @type { min: number, max: number }
   * @description
   * Sparkle animation duration limits
   */
  duration?: Range;
  /**
   * @default "fill-only"
   * @type "stroke-only" | "fill-only" | "fill-and-stroke"
   * @description
   * Icon render type
   */
  fillType?: "stroke-only" | "fill-only" | "fill-and-stroke";
  /**
   * @default { x: { min: -10, max: 100 }, y: { min: -10, max: 75 } }
   * @type { x: { min: number, max: number }, y: { min: number, max: number } }
   * @description
   * Sparkle position range as a percentage, negative values supported
   */
  offset?: { x?: Range; y?: Range };
  /**
   * @default 1
   * @type number
   * @description
   * Maximum opacity | 1 === 100%
   */
  opacity?: number;
  /**
   * @default [-45, 45]
   * @type number[]
   * @description
   * Rotation to move through for each sparkle
   */
  rotate?: number[];
  /**
   * @default { min: 0.3, max: 1.3 }
   * @type { min: number, max: number }
   * @description
   * Sparkle scale limits with 1 = 100%
   */
  scale?: Range;
  sparklesCount?: number;
};

export const Sparkles = ({
  children,
  colors,
  duration = { max: 1.2, min: 0.4 },
  fillType = "fill-only",
  icon,
  offset = { x: { max: 100, min: -10 }, y: { max: 75, min: -10 } },
  opacity = 1,
  rotate = [-45, 45],
  scale = { max: 1.3, min: 0.3 },
  sparklesCount = 5,
}: SparklesProps) => {
  return (
    <div>
      <span className="relative inline-block">
        {SPARKLE_KEYS.slice(0, sparklesCount).map((sparkleKey) => (
          <Sparkle
            colors={colors ?? []}
            duration={duration}
            fillType={fillType}
            icon={icon}
            key={sparkleKey}
            offset={offset}
            opacity={opacity}
            rotate={rotate}
            scale={scale}
          />
        ))}
        {children}
      </span>
    </div>
  );
};
