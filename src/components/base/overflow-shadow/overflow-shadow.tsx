// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { OverlayScrollbarsComponent } from "overlayscrollbars-react";
import { useRef } from "react";
import { VariantProps } from "tailwind-variants";

import { cn } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

import type { OverlayScrollbars } from "overlayscrollbars";

const overflowShadowVariants = tv({
  compoundSlots: [
    {
      class: "absolute z-100 from-content-fg/25 to-transparent",
      slots: ["end", "start"],
    },
    {
      class: "w-full h-[10px]",
      orientation: "vertical",
      slots: ["end", "start"],
    },
    {
      class: "w-[10px] inset-y-0",
      orientation: "horizontal",
      slots: ["end", "start"],
    },
  ],
  compoundVariants: [
    {
      class: {
        end: "rounded-r-md",
        start: "rounded-l-md",
      },
      orientation: "horizontal",
      shadowRadius: "md",
    },
    {
      class: {
        end: "rounded-r-sm",
        start: "rounded-l-sm",
      },
      orientation: "horizontal",
      shadowRadius: "sm",
    },
    {
      class: {
        end: "rounded-b-md",
        start: "rounded-t-md",
      },
      orientation: "vertical",
      shadowRadius: "md",
    },
    {
      class: {
        end: "rounded-b-sm",
        start: "rounded-t-sm",
      },
      orientation: "vertical",
      shadowRadius: "sm",
    },
  ],
  defaultVariants: {
    orientation: "vertical",
    shadowRadius: "sm",
  },
  slots: {
    end: "pointer-events-none",
    os: "w-full h-full relative overflow-hidden",
    start: "pointer-events-none",
  },
  variants: {
    insetShadow: {
      true: {
        os: "inset-shadow-full",
      },
    },
    orientation: {
      horizontal: {
        end: "right-0 bg-gradient-to-l",
        start: "left-0 bg-gradient-to-r",
      },
      vertical: {
        end: "bottom-0 bg-gradient-to-t",
        start: "top-0 bg-gradient-to-b",
      },
    },
    shadowRadius: {
      md: {
        os: "rounded-md",
      },
      none: {
        os: "rounded-none",
      },
      sm: {
        os: "rounded-sm",
      },
    },
  },
});

type OverflowShadowProps = VariantProps<typeof overflowShadowVariants> & {
  children?: React.ReactNode;
  className?: string;
  hideScrollbar?: boolean;
  rootClassName?: string;
  startAtEnd?: boolean;
};

export const OverflowShadow = ({
  children,
  className,
  hideScrollbar,
  insetShadow,
  orientation,
  rootClassName,
  shadowRadius,
  startAtEnd,
}: OverflowShadowProps) => {
  const { end, os, start } = overflowShadowVariants({
    insetShadow,
    orientation,
    shadowRadius,
  });

  const startRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);

  const updateShadows = (instance: OverlayScrollbars) => {
    const { viewport } = instance.elements();
    if (!startRef.current || !endRef.current) return;

    const scrollPosition =
      orientation === "horizontal" ? viewport.scrollLeft : viewport.scrollTop;
    const scrollSize =
      orientation === "horizontal"
        ? viewport.scrollWidth
        : viewport.scrollHeight;
    const clientSize =
      orientation === "horizontal"
        ? viewport.clientWidth
        : viewport.clientHeight;
    const maxScroll = scrollSize - clientSize;

    if (maxScroll > 0) {
      const scrollAmount = scrollPosition / maxScroll;
      startRef.current.style.opacity = scrollAmount.toString();
      endRef.current.style.opacity = (1 - scrollAmount).toString();
    } else {
      startRef.current.style.opacity = "0";
      endRef.current.style.opacity = "0";
    }
  };

  const initialize = (instance: OverlayScrollbars) => {
    const { viewport } = instance.elements();

    if (startAtEnd) {
      if (orientation === "horizontal") {
        viewport.scrollLeft = viewport.scrollWidth;
      } else {
        viewport.scrollTop = viewport.scrollHeight;
      }
    }

    updateShadows(instance);
  };

  return (
    <div className={cn(os(), rootClassName)}>
      <OverlayScrollbarsComponent
        className="w-full h-full"
        defer
        events={{
          initialized: initialize,
          scroll: updateShadows,
          updated: updateShadows,
        }}
        options={{
          overflow: {
            x: orientation === "horizontal" ? "scroll" : "hidden",
            y: orientation === "horizontal" ? "hidden" : "scroll",
          },
          scrollbars: {
            autoHide: "scroll",
            theme: "os-theme-orbit-capture",
            visibility: hideScrollbar ? "hidden" : "visible",
          },
        }}
      >
        <div
          className={cn(
            orientation === "horizontal" && "text-nowrap",
            className,
          )}
        >
          {children}
        </div>
      </OverlayScrollbarsComponent>

      <div className={start()} ref={startRef} />
      <div className={end()} ref={endRef} />
    </div>
  );
};
