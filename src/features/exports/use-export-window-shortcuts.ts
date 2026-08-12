// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { ownsTextEditingKeys } from "./keyboard-target";

const INTERACTIVE_SELECTOR = [
  "a[href]",
  "button",
  "input",
  "select",
  "textarea",
  "[contenteditable='true']",
  "[role='button']",
  "[role='checkbox']",
  "[role='radio']",
  "[role='slider']",
  "[role='switch']",
].join(",");

const hasInteractiveTarget = (target: EventTarget | null) =>
  target instanceof Element && target.closest(INTERACTIVE_SELECTOR) !== null;

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

      if (
        event.code === "Space" &&
        onTogglePlayback &&
        !hasInteractiveTarget(event.target)
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

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onToggleCrop, onTogglePlayback]);
}
