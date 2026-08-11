// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { motion } from "motion/react";
import { ReactNode, useId } from "react";
import {
  Selection,
  ToggleButton,
  ToggleButtonGroup,
} from "react-aria-components";

import { cn, elementFocusVisible } from "../../../lib/styling";

export type PillGroupItem = {
  id: string;
  label: string;
  ariaLabel?: string;
  icon?: ReactNode;
};

const selectedId = (selection: Selection) => {
  if (selection === "all") return null;
  const first = selection.values().next().value;
  return typeof first === "string" ? first : null;
};

export function PillGroup({
  ariaLabel,
  className,
  display = "icon",
  isDisabled,
  items,
  onSelectionChange,
  selected,
}: {
  ariaLabel: string;
  items: PillGroupItem[];
  onSelectionChange: (id: string) => void;
  selected: string;
  className?: string;
  display?: "icon" | "icon-label" | "label";
  isDisabled?: boolean;
}) {
  const selectionId = useId();

  return (
    <ToggleButtonGroup
      aria-label={ariaLabel}
      className={cn("flex items-center gap-1", className)}
      disallowEmptySelection
      isDisabled={isDisabled}
      onSelectionChange={(selection) => {
        const id = selectedId(selection);
        if (id !== null) onSelectionChange(id);
      }}
      selectedKeys={new Set([selected])}
      selectionMode="single"
    >
      {items.map((item) => (
        <ToggleButton
          aria-label={item.ariaLabel ?? item.label}
          className={cn(
            "relative flex h-7 items-center justify-center rounded-md px-2 text-xs text-muted outline-none transition-colors data-[hovered]:text-content-fg data-[selected]:text-content-fg",
            elementFocusVisible,
            display === "icon" && "w-7 px-0",
          )}
          id={item.id}
          key={item.id}
        >
          {({ isSelected }) => (
            <>
              {isSelected ? (
                <motion.span
                  aria-hidden="true"
                  className="absolute inset-0 transform-gpu rounded-md border border-muted/30 bg-neutral backface-hidden will-change-transform"
                  layoutId={`pill-selection-${selectionId}`}
                  transition={{ damping: 25, stiffness: 350, type: "spring" }}
                />
              ) : null}
              <motion.span
                animate={{ opacity: isSelected ? 1 : 0.75 }}
                className="relative z-10 flex min-w-0 origin-center transform-gpu items-center justify-center gap-1.5 whitespace-nowrap backface-hidden will-change-transform"
                initial={false}
                transition={{ duration: 0.12 }}
              >
                {display !== "label" ? item.icon : null}
                {display !== "icon" ? item.label : null}
              </motion.span>
            </>
          )}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );
}
