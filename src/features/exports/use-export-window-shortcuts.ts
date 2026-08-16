// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { ownsTextEditingKeys } from "./keyboard-target";

export function useExportWindowShortcuts({
  onCopy,
  onDelete,
  onExport,
  onMoveBackward,
  onMoveForward,
  onResizeCanvas,
  onSelectTool,
  onToggleCrop,
  onTogglePlayback,
}: {
  onCopy?: () => void;
  onDelete?: () => void;
  onExport?: () => void;
  onMoveBackward?: () => void;
  onMoveForward?: () => void;
  onResizeCanvas?: () => void;
  onSelectTool?: () => void;
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

      if (
        (event.code === "Backspace" || event.code === "Delete") &&
        onDelete &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onDelete();
        return;
      }

      if (
        event.code === "BracketLeft" &&
        onMoveForward &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onMoveForward();
        return;
      }

      if (
        event.code === "BracketRight" &&
        onMoveBackward &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onMoveBackward();
        return;
      }

      // P leaves Space available to activate whichever control has focus.
      if (
        event.code === "KeyP" &&
        onTogglePlayback &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onTogglePlayback();
      } else if (
        event.code === "KeyF" &&
        onResizeCanvas &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onResizeCanvas();
      } else if (
        event.code === "KeyC" &&
        onToggleCrop &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onToggleCrop();
      } else if (
        event.code === "KeyV" &&
        onSelectTool &&
        !ownsTextEditingKeys(event.target)
      ) {
        event.preventDefault();
        onSelectTool();
      }
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [
    onCopy,
    onDelete,
    onExport,
    onMoveBackward,
    onMoveForward,
    onResizeCanvas,
    onSelectTool,
    onToggleCrop,
    onTogglePlayback,
  ]);
}
