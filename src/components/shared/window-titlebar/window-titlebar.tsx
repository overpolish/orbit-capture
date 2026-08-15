// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Minus, X } from "lucide-react";
import { ReactNode } from "react";

import logoUrl from "../../../assets/screenwide-mark.svg";
import { Button } from "../../base/button/button";

export function WindowTitlebar({
  actions,
  border = true,
  center,
  onClose,
  onMinimize,
  title,
}: {
  actions?: ReactNode;
  border?: boolean;
  center?: ReactNode;
  onClose?: () => void;
  onMinimize?: () => void;
  title?: string;
}) {
  return (
    <header
      className={`relative flex h-12 min-w-0 shrink-0 items-center gap-2 px-3 ${border ? "border-b border-muted/15" : ""}`}
      data-tauri-drag-region="deep"
    >
      <img
        alt="Screenwide"
        className="pointer-events-none size-5 shrink-0 brightness-0 dark:invert"
        data-tauri-drag-region
        draggable={false}
        src={logoUrl}
      />
      {title ? (
        <span
          className="pointer-events-none text-sm font-semibold"
          data-tauri-drag-region
        >
          {title}
        </span>
      ) : null}
      {center ? (
        <div className="absolute left-1/2 -translate-x-1/2">{center}</div>
      ) : null}
      <div className="min-w-4 grow" data-tauri-drag-region />
      {actions}
      <Button
        aria-label="Minimize"
        className="group"
        icon
        onPress={onMinimize}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <Minus
          className="transform-gpu text-muted transition-[color,transform] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg"
          size={18}
        />
      </Button>
      <Button
        aria-label="Close"
        className="group"
        icon
        onPress={onClose}
        showFocus={false}
        size="sm"
        variant="ghost"
      >
        <X
          className="transform-gpu text-muted transition-[color,transform] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg"
          size={18}
        />
      </Button>
    </header>
  );
}
