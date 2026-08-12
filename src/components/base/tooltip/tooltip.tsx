// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { clsx } from "clsx";
import {
  Tooltip as AriaTooltip,
  TooltipProps as AriaTooltipProps,
  OverlayArrow,
} from "react-aria-components";
import { VariantProps } from "tailwind-variants";

import { tv } from "../../../lib/variants";

const tooltipVariants = tv({
  base: [
    "bg-content-fg text-content m-2 rounded-sm shadow-md",
    "data-entering:animate-in data-entering:fade-in",
    "data-exiting:animate-out data-exiting:fade-out",
  ],
  defaultVariants: {
    size: "sm",
  },
  variants: {
    size: {
      md: "text-md py-1 px-2",
      sm: "text-sm py-1 px-2",
    },
  },
});

type TooltipProps = AriaTooltipProps &
  VariantProps<typeof tooltipVariants> & {
    children?: React.ReactNode;
    className?: string;
    withArrow?: boolean;
  };

export const Tooltip = ({
  children,
  className,
  size,
  withArrow = true,
  ...props
}: TooltipProps) => {
  return (
    <AriaTooltip {...props} className={tooltipVariants({ className, size })}>
      {withArrow && (
        <OverlayArrow>
          {({ placement }) => {
            const resolvedPlacement = placement ?? props.placement ?? "top";
            return (
              <svg
                className={clsx(
                  "fill-content-fg",
                  (resolvedPlacement.startsWith("left") ||
                    resolvedPlacement.startsWith("start")) &&
                    "rotate-270",
                  (resolvedPlacement.startsWith("right") ||
                    resolvedPlacement.startsWith("end")) &&
                    "rotate-90",
                  resolvedPlacement.startsWith("bottom") && "rotate-180",
                )}
                height={8}
                viewBox="0 0 8 8"
                width={8}
              >
                <path d="M0 0 L4 4 L8 0" />
              </svg>
            );
          }}
        </OverlayArrow>
      )}

      {children}
    </AriaTooltip>
  );
};
