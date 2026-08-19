// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { ownsArrowKeys, ownsTextEditingKeys } from "./keyboard-target";

const arrowDirections = new Map([
  ["ArrowDown", { x: 0, y: 1 }],
  ["ArrowLeft", { x: -1, y: 0 }],
  ["ArrowRight", { x: 1, y: 0 }],
  ["ArrowUp", { x: 0, y: -1 }],
]);

export function useExportWindowShortcuts({
  onCopy,
  onDelete,
  onExport,
  onMoveBackward,
  onMoveForward,
  onNudge,
  onResizeCanvas,
  onSelectTool,
  onStep,
  onToggleCrop,
  onTogglePlayback,
}: {
  onCopy?: () => void;
  onDelete?: () => void;
  onExport?: () => void;
  onMoveBackward?: () => void;
  onMoveForward?: () => void;
  /** Moves the selected layer by one arrow press; `coarse` is the Shift jump. */
  onNudge?: (directionX: number, directionY: number, coarse: boolean) => void;
  onResizeCanvas?: () => void;
  onSelectTool?: () => void;
  /** Moves the playhead by one arrow press; `coarse` is the Shift jump. */
  onStep?: (direction: -1 | 1, coarse: boolean) => void;
  onToggleCrop?: () => void;
  onTogglePlayback?: () => void;
}) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // Arrows run before the shared guards: holding one has to repeat, and
      // Shift only picks the bigger jump rather than naming another shortcut.
      const arrow = arrowDirections.get(event.code);
      if (arrow) {
        if (
          event.isComposing ||
          event.altKey ||
          event.ctrlKey ||
          event.metaKey ||
          ownsTextEditingKeys(event.target) ||
          ownsArrowKeys(event.target)
        )
          return;
        const handled = onNudge
          ? () => {
              onNudge(arrow.x, arrow.y, event.shiftKey);
            }
          : onStep && arrow.x !== 0
            ? () => {
                onStep(arrow.x > 0 ? 1 : -1, event.shiftKey);
              }
            : null;
        if (!handled) return;
        event.preventDefault();
        // The timeline ruler keeps its own arrow handler for when it has focus;
        // stop the event here so a bubbling copy cannot seek a second time.
        event.stopPropagation();
        handled();
        return;
      }

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
    onNudge,
    onResizeCanvas,
    onSelectTool,
    onStep,
    onToggleCrop,
    onTogglePlayback,
  ]);
}
