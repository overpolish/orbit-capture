// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Lock } from "lucide-react";
import { MouseEventHandler } from "react";

import { cn } from "../../../lib/styling";

export function ColorSwatch({
  ariaLabel,
  isDisabled,
  isLocked,
  onChange,
  onContextMenu,
  value,
}: {
  ariaLabel: string;
  value: string;
  isDisabled?: boolean;
  isLocked?: boolean;
  onChange?: (value: string) => void;
  onContextMenu?: MouseEventHandler<HTMLSpanElement>;
}) {
  return (
    <span
      className={cn(
        "relative inline-block size-5 shrink-0 overflow-hidden rounded-sm border border-muted/30 align-middle",
        isDisabled && "cursor-not-allowed",
      )}
      onContextMenu={onContextMenu}
      style={{ backgroundColor: value }}
    >
      <input
        aria-label={ariaLabel}
        className={cn(
          "absolute inset-0 size-full opacity-0",
          isDisabled ? "cursor-not-allowed" : "cursor-pointer",
        )}
        disabled={isDisabled}
        onChange={(event) => {
          onChange?.(event.currentTarget.value.toUpperCase());
        }}
        type="color"
        value={value}
      />
      {isLocked ? (
        <span className="pointer-events-none absolute inset-0 flex items-center justify-center bg-black/25 text-white drop-shadow-sm">
          <Lock aria-hidden="true" size={9} strokeWidth={2.5} />
        </span>
      ) : null}
    </span>
  );
}
