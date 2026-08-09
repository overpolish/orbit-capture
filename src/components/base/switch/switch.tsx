// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { motion, useMotionValue, Variants } from "motion/react";
import { useState } from "react";
import {
  SwitchButton as AriaSwitchButton,
  SwitchField as AriaSwitchField,
  SwitchFieldProps as AriaSwitchFieldProps,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { focusStyles, groupFocusVisible } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

const switchVariants = tv({
  defaultVariants: {
    color: "success",
    size: "md",
  },
  slots: {
    base: "group flex gap-2 items-center text-content-fg outline-none",
    container: [
      "relative flex cursor-default rounded-full shadow-inner bg-neutral transition-colors items-center border-1 border-neutral/5 transition-all",
      focusStyles,
      groupFocusVisible,
    ],
    innerLabel:
      "absolute text-black font-black opacity-0 group-data-[selected]:opacity-20 transition-all",
    thumb: "rounded-full bg-white shadow-sm shadow-inner z-1",
  },
  variants: {
    color: {
      success: {
        container: "group-data-[selected]:bg-success",
      },
    },
    size: {
      md: {
        base: "text-sm",
        container: "h-[26px] w-[46px] p-[3px]",
        innerLabel: "text-xxs left-0.75",
        thumb: "h-[20px]",
      },
      xs: {
        base: "text-xs",
        container: "h-[17px] w-[28px] p-[1px]",
        innerLabel: "text-[4px] left-0.75",
        thumb: "h-[13px]",
      },
    },
  },
});

type SwitchProps = AriaSwitchFieldProps &
  VariantProps<typeof switchVariants> & {
    children?: React.ReactNode;
    className?: string;
  };

const sizeToWidth = (
  size: SwitchProps["size"],
): { animate: number; rest: number } => {
  if (size === "md") return { animate: 22, rest: 20 };
  if (size === "xs") return { animate: 15, rest: 13 };
  return { animate: 22, rest: 20 };
};

export const Switch = ({
  children,
  className,
  size,
  ...props
}: SwitchProps) => {
  const { base, container, innerLabel, thumb } = switchVariants({ size });
  // Track if animating, otherwise initial render and tap cancel cause animate to run
  const [isAnimating, setIsAnimating] = useState(false);

  const { animate, rest } = sizeToWidth(size);
  // Allows smooth start for animation
  const width = useMotionValue(rest);
  const thumbAnimations: Variants = {
    motion: { width: [width.get(), animate, rest] },
    rest: { width: rest },
    tapped: { width: animate },
  };

  return (
    <AriaSwitchField {...props} className={base({ className })}>
      {children}
      <AriaSwitchButton className="contents">
        {({ isPressed, isSelected }) => (
          <motion.div
            className={container()}
            layout
            style={{ justifyContent: isSelected ? "flex-end" : "flex-start" }}
          >
            <span className={innerLabel()}>ON</span>
            <motion.div
              animate={isAnimating ? "motion" : isPressed ? "tapped" : "rest"}
              className={thumb()}
              layout
              onLayoutAnimationComplete={() => {
                setIsAnimating(false);
              }}
              onLayoutAnimationStart={() => {
                setIsAnimating(true);
              }}
              style={{ width }}
              transition={{ duration: 0.2 }}
              variants={thumbAnimations}
            />
          </motion.div>
        )}
      </AriaSwitchButton>
    </AriaSwitchField>
  );
};
