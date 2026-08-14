// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { ownsTextEditingKeys } from "./keyboard-target";

export function useExportWindowShortcuts({
  onCopy,
  onExport,
  onToggleCrop,
  onTogglePlayback,
}: {
  onCopy?: () => void;
  onExport?: () => void;
  onToggleCrop?: () => void;
  onTogglePlayback?: () => void;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || event.isComposing || event.altKey) return;

      const commandKey = event.ctrlKey || event.metaKey;
      if (commandKey && !event.shiftKey) {
        if (event.code === "KeyC" && onCopy) {
          if (ownsTextEditingKeys(event.target)) return;
          event.preventDefault();
          onCopy();
        } else if (event.code === "KeyE" && onExport) {
          event.preventDefault();
          onExport();
        }
        return;
      }

      if (event.ctrlKey || event.metaKey || event.shiftKey) return;

      // P leaves Space available to activate whichever control has focus.
      if (
        event.code === "KeyP" &&
        onTogglePlayback &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onTogglePlayback();
      } else if (
        event.code === "KeyC" &&
        onToggleCrop &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onToggleCrop();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [onCopy, onExport, onToggleCrop, onTogglePlayback]);
}
