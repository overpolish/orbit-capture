// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { motion } from "motion/react";
import {
  CheckboxButton as AriaCheckboxButton,
  CheckboxField as AriaCheckboxField,
  type CheckboxFieldProps as AriaCheckboxFieldProps,
} from "react-aria-components";

import { focusStyles, groupFocusVisible } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

import type { VariantProps } from "tailwind-variants";

const checkboxVariants = tv({
  defaultVariants: {
    size: "md",
  },
  slots: {
    base: [
      "group relative flex items-center gap-2 text-sm text-content-fg",
      focusStyles,
    ],
    checkbox: [
      "flex shrink-0 items-center justify-center rounded-sm border-1 border-muted/50 transition-colors",
      "group-data-[hovered]:bg-info/10",
      "group-data-[selected]:border-info group-data-[selected]:bg-info",
      groupFocusVisible,
    ],
    svg: "fill-none",
  },
  variants: {
    disabled: {
      true: {
        base: "cursor-not-allowed",
        checkbox: [
          "border-muted bg-muted",
          "group-data-[selected]:border-muted group-data-[selected]:bg-muted",
        ],
      },
    },
    size: {
      md: {
        checkbox: "size-5",
        svg: "size-3.5 translate-y-[0.5px]",
      },
      sm: {
        checkbox: "size-4",
        svg: "size-3",
      },
      xs: {
        checkbox: "size-3.5",
        svg: "size-2.5",
      },
    },
  },
});

type CheckboxProps = Omit<AriaCheckboxFieldProps, "children"> &
  VariantProps<typeof checkboxVariants> & {
    children?: React.ReactNode;
  };

export const Checkbox = ({ children, size, ...props }: CheckboxProps) => {
  const { base, checkbox, svg } = checkboxVariants({
    disabled: props.isDisabled,
    size,
  });

  return (
    <AriaCheckboxField {...props} className="contents">
      <AriaCheckboxButton className={base()}>
        {({ isSelected }) => (
          <>
            <div className={checkbox()}>
              <svg aria-hidden="true" className={svg()} viewBox="3 4 12 10">
                <motion.path
                  animate={{
                    opacity: isSelected ? 1 : 0,
                    pathLength: isSelected ? 1 : 0,
                  }}
                  d="M4 9 L7 12 L14 5"
                  initial={false}
                  stroke="white"
                  strokeLinecap="round"
                  strokeWidth="2"
                  transition={{
                    duration: 0.2,
                    // A round linecap leaves a dot at pathLength 0 unless its
                    // opacity is also taken away after the path retracts.
                    opacity: {
                      delay: isSelected ? 0 : 0.15,
                      duration: 0.05,
                      ease: "easeInOut",
                    },
                  }}
                />
              </svg>
            </div>
            {children}
          </>
        )}
      </AriaCheckboxButton>
    </AriaCheckboxField>
  );
};
