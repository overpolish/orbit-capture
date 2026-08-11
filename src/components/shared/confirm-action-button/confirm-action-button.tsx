// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ReactNode, useEffect, useRef, useState } from "react";

import { ToggleButton } from "../../base/button/toggle-button";

const DEFAULT_CONFIRM_TIMEOUT_MS = 2_000;

export function ConfirmActionButton({
  armedIcon,
  armedLabel,
  className,
  idleIcon,
  idleLabel,
  isDisabled,
  onConfirm,
  timeoutMs = DEFAULT_CONFIRM_TIMEOUT_MS,
}: {
  armedIcon: ReactNode;
  armedLabel: string;
  idleIcon: ReactNode;
  idleLabel: string;
  className?: string;
  isDisabled?: boolean;
  onConfirm?: () => void;
  timeoutMs?: number;
}) {
  const [isArmed, setIsArmed] = useState(false);
  const disarmRef = useRef<number | undefined>(undefined);

  useEffect(
    () => () => {
      window.clearTimeout(disarmRef.current);
    },
    [],
  );

  return (
    <ToggleButton
      aria-label={isArmed ? armedLabel : idleLabel}
      className={className}
      isDisabled={isDisabled}
      isSelected={isArmed}
      off={idleIcon}
      onChange={(selected) => {
        window.clearTimeout(disarmRef.current);

        if (!selected) {
          setIsArmed(false);
          onConfirm?.();
          return;
        }

        setIsArmed(true);
        disarmRef.current = window.setTimeout(() => {
          setIsArmed(false);
        }, timeoutMs);
      }}
      showFocus={false}
      variant="ghost"
    >
      {armedIcon}
    </ToggleButton>
  );
}
