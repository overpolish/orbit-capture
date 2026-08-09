// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Lock } from "lucide-react";
import { ReactNode } from "react";

import { ToggleButton } from "../../../components/base/button/toggle-button";

type RecordingBarInputToggleProps = {
  isSelected: boolean;
  label: string;
  off: ReactNode;
  on: ReactNode;
  onChange: (isSelected: boolean) => void;
  isDisabled?: boolean;
  isLocked?: boolean;
  onLockedPress?: () => void;
};

export function RecordingBarInputToggle({
  isDisabled,
  isLocked,
  isSelected,
  label,
  off,
  on,
  onChange,
  onLockedPress,
}: RecordingBarInputToggleProps) {
  return (
    <div className="relative flex justify-center">
      {isLocked && !isDisabled ? (
        <Lock className="absolute -top-3 text-muted" size={12} />
      ) : null}
      <ToggleButton
        aria-label={label}
        className="data-[disabled]:opacity-35"
        isDisabled={isDisabled}
        isSelected={isSelected}
        off={off}
        onChange={(selected) => {
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
