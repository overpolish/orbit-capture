// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { clsx, ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...classes: ClassValue[]) {
  return twMerge(clsx(classes));
}

export function availableVariants<T extends readonly string[]>(
  ...keys: T
): Record<T[number], string> {
  return Object.fromEntries(keys.map((key) => [key, ""])) as Record<
    T[number],
    string
  >;
}

export const focusStyles =
  "outline-none ring-content-fg/75 ring-offset-content transition-[box-shadow,background-color,color,border-color]";

// Interactive elements where focus is not required on non-keyboard interaction, e.g., buttons
export const elementFocusVisible =
  "data-[focus-visible]:ring-offset-1 data-[focus-visible]:ring-1";

export const groupFocusVisible =
  "group-data-[focus-visible]:ring-offset-1 group-data-[focus-visible]:ring-1";

export const focusWithin =
  "data-[focus-within]:ring-offset-1 data-[focus-within]:ring-1";
