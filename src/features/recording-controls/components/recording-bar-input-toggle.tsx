// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Lock, TriangleAlert } from "lucide-react";
import { ReactNode } from "react";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { cn } from "../../../lib/styling";

type RecordingBarInputToggleProps = {
  isSelected: boolean;
  label: string;
  off: ReactNode;
  on: ReactNode;
  onChange: (isSelected: boolean) => void;
  hasWarning?: boolean;
  isDisabled?: boolean;
  isLocked?: boolean;
  isReadOnly?: boolean;
  onLockedPress?: () => void;
  warningLabel?: string;
};

export function RecordingBarInputToggle({
  hasWarning,
  isDisabled,
  isLocked,
  isReadOnly,
  isSelected,
  label,
  off,
  on,
  onChange,
  onLockedPress,
  warningLabel,
}: RecordingBarInputToggleProps) {
  return (
    <div className="relative flex justify-center">
      {hasWarning && isSelected && !isDisabled ? (
        <TriangleAlert
          aria-label={warningLabel ?? `${label} source is not detected`}
          className="absolute -top-3 text-warning"
          role="img"
          size={12}
        />
      ) : isLocked && !isDisabled ? (
        <Lock className="absolute -top-3 text-muted" size={12} />
      ) : null}
      <ToggleButton
        aria-disabled={isReadOnly || undefined}
        aria-label={label}
        className={cn(
          "data-[disabled]:opacity-35",
          isReadOnly &&
            "pointer-events-none cursor-default data-[hovered]:scale-100",
        )}
        isDisabled={isDisabled}
        isSelected={isSelected}
        off={off}
        onChange={(selected) => {
          if (isReadOnly) return;
          if (isLocked) {
            onLockedPress?.();
          } else {
            onChange(selected);
          }
        }}
        size="sm"
        variant="ghost"
      >
        {on}
      </ToggleButton>
    </div>
  );
}
