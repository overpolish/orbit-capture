// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useRef } from "react";

import { CameraOverlaySettings } from "../types";

const HISTORY_LIMIT = 50;

const sameSettings = (
  left: CameraOverlaySettings,
  right: CameraOverlaySettings,
) =>
  Object.keys(left).every(
    (key) =>
      left[key as keyof CameraOverlaySettings] ===
      right[key as keyof CameraOverlaySettings],
  );

const ownsTextUndo = (target: EventTarget | null) =>
  target instanceof HTMLInputElement ||
  target instanceof HTMLTextAreaElement ||
  (target instanceof HTMLElement && target.isContentEditable);

export const useCameraOverlayHistory = ({
  enabled,
  onChange,
  resetKey,
  settings,
}: {
  enabled: boolean;
  resetKey: string | number;
  settings: CameraOverlaySettings;
  onChange?: (settings: CameraOverlaySettings) => void;
}) => {
  const currentRef = useRef(settings);
  const gestureStartRef = useRef<CameraOverlaySettings | undefined>(undefined);
  const historyRef = useRef<{
    future: CameraOverlaySettings[];
    past: CameraOverlaySettings[];
  }>({ future: [], past: [] });
  const onChangeRef = useRef(onChange);
  currentRef.current = settings;
  onChangeRef.current = onChange;

  const apply = useCallback((next: CameraOverlaySettings) => {
    currentRef.current = next;
    onChangeRef.current?.(next);
  }, []);

  const undo = useCallback(() => {
    const previous = historyRef.current.past.pop();
    if (!previous) return;
    historyRef.current.future.push(currentRef.current);
    apply(previous);
  }, [apply]);

  const redo = useCallback(() => {
    const next = historyRef.current.future.pop();
    if (!next) return;
    historyRef.current.past.push(currentRef.current);
    apply(next);
  }, [apply]);

  useEffect(() => {
    historyRef.current = { future: [], past: [] };
    gestureStartRef.current = undefined;
  }, [resetKey]);

  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (ownsTextUndo(event.target)) return;
      const modifier = event.metaKey || event.ctrlKey;
      const isUndo = modifier && event.key.toLowerCase() === "z";
      const isRedo =
        (isUndo && event.shiftKey) ||
        (event.ctrlKey && event.key.toLowerCase() === "y");
      if (!isUndo && !isRedo) return;
      event.preventDefault();
      if (isRedo) redo();
      else undo();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [enabled, redo, undo]);

  return {
    beginGesture: () => {
      gestureStartRef.current = currentRef.current;
    },
    change: apply,
    endGesture: () => {
      const start = gestureStartRef.current;
      gestureStartRef.current = undefined;
      if (!start || sameSettings(start, currentRef.current)) return;
      historyRef.current.past.push(start);
      if (historyRef.current.past.length > HISTORY_LIMIT)
        historyRef.current.past.shift();
      historyRef.current.future = [];
    },
  };
};
