// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { focusStyles, focusWithin } from "../../../lib/styling";
import { tv } from "../../../lib/variants";

export const inputFieldVariants = tv({
  compoundVariants: [
    { class: { input: "py-1.5" }, size: "md", variant: "line" },
    { class: { input: "py-1.5" }, size: "sm", variant: "line" },
    { class: { inputWrapper: "px-1" }, variant: "line" },
    {
      class: {
        field: "border border-error focus-within:border-error ring-error/75",
      },
      isInvalid: true,
      variant: "solid",
    },
    {
      class: { line: "shadow-error group-data-[focus-within]:shadow-error" },
      isInvalid: true,
      variant: "line",
    },
  ],
  defaultVariants: {
    size: "md",
    variant: "solid",
  },
  slots: {
    base: "flex flex-col gap-1 w-full",
    field: "group relative flex flex-row items-center",
    input: [
      "text-content-fg outline-none w-full",
      "placeholder:font-extralight placeholder:italic",
    ],
    inputWrapper:
      "outline-none text-muted/75 flex flex-row items-center justify-between w-full gap-2",
    label: "text-muted font-medium tabular-nums",
    line: [
      "absolute bottom-0 inset-x-0 bg-transparent h-[2px] pointer-events-none transition-shadow shadow-[0_1px_0_0] shadow-muted/30",
      "group-data-[focus-within]:shadow-[0_2px_0_0] group-data-[focus-within]:shadow-content-fg/75",
    ],
  },
  variants: {
    centered: { true: { input: "text-center" } },
    isInvalid: { true: "" },
    size: {
      md: {
        input: "text-sm py-2 placeholder:text-xs",
        inputWrapper: "px-3 gap-3",
        label: "text-sm",
      },
      sm: {
        input: "text-xs py-2 placeholder:text-xxs",
        inputWrapper: "px-2 gap-2",
        label: "text-xs",
      },
    },
    variant: {
      line: {},
      solid: {
        field: ["border border-muted/30 rounded-md", focusStyles, focusWithin],
      },
    },
  },
});
