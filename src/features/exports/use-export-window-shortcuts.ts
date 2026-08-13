// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { ownsTextEditingKeys } from "./keyboard-target";

export function useExportWindowShortcuts({
  onToggleCrop,
  onTogglePlayback,
}: {
  onToggleCrop?: () => void;
  onTogglePlayback?: () => void;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.repeat ||
        event.isComposing ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey
      )
        return;

      // Space is transport control everywhere except a text field, exactly
      // like a video editor: clicking a checkbox must not steal playback.
      if (
        event.code === "Space" &&
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

    // Focused controls (checkboxes, buttons) activate on space keyup;
    // suppressing it keeps the toggle from firing after playback handled the
    // keydown.
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code === "Space" && !ownsTextEditingKeys(event.target)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [onToggleCrop, onTogglePlayback]);
}
