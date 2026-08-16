// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
} from "react";

import { ownsTextEditingKeys } from "./keyboard-target";
import {
  RecordingOutputSettings,
  ScreenshotWorkspaceOutputSettings,
} from "./screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
  RecordingVideoTrackId,
} from "./types";

const HISTORY_LIMIT = 100;
const GROUP_DELAY_MS = 300;

type ExportEditGesture = {
  beginGesture: () => void;
  endGesture: () => void;
};

export const ExportEditGestureContext = createContext<ExportEditGesture>({
  beginGesture: () => undefined,
  endGesture: () => undefined,
});

export const useExportEditGesture = () => use(ExportEditGestureContext);

export type ExportEditState = {
  audioTrackVolumes: {
    artifactId: number;
    values: AudioTrackVolume[];
  } | null;
  bakeCamera: boolean;
  cameraCompression: number;
  cameraOverlay: CameraOverlaySettings;
  cameraResolutionScalePercent: number;
  collapseAudio: boolean;
  compression: number;
  cursorEffects: CursorEffectSettings;
  recordingOutput: RecordingOutputSettings;
  resolutionScalePercent: number;
  screenshotOutput: ScreenshotWorkspaceOutputSettings;
  trackSelection: { artifactId: number; streamIndices: number[] } | null;
  videoTrackSelection: {
    artifactId: number;
    tracks: RecordingVideoTrackId[];
  } | null;
};

const changedKey = <State extends object>(before: State, after: State) =>
  Object.keys(after).find(
    (key) => before[key as keyof State] !== after[key as keyof State],
  );

/** One undo stack for every option that changes the exported result. */
export function useExportEditHistory<State extends object>({
  apply,
  resetKey,
  state,
}: {
  apply: (state: State) => void;
  resetKey: unknown;
  state: State;
}) {
  const applyRef = useRef(apply);
  const currentRef = useRef(state);
  const observedRef = useRef(state);
  const futureRef = useRef<State[]>([]);
  const pastRef = useRef<State[]>([]);
  const pendingRef = useRef<{
    key: string | undefined;
    start: State;
  } | null>(null);
  const timerRef = useRef<number | null>(null);
  const applyingRef = useRef(false);
  const gestureRef = useRef(false);
  const suppressRef = useRef(true);
  applyRef.current = apply;
  currentRef.current = state;

  const finishGroup = useCallback(() => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    const pending = pendingRef.current;
    pendingRef.current = null;
    if (!pending) return;
    if (pending.start === currentRef.current) return;
    pastRef.current.push(pending.start);
    if (pastRef.current.length > HISTORY_LIMIT) pastRef.current.shift();
    futureRef.current = [];
  }, []);

  useEffect(() => {
    suppressRef.current = true;
    pastRef.current = [];
    futureRef.current = [];
    pendingRef.current = null;
    gestureRef.current = false;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    observedRef.current = currentRef.current;
    const frame = requestAnimationFrame(() => {
      observedRef.current = currentRef.current;
      suppressRef.current = false;
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [resetKey]);

  useEffect(() => {
    const previous = observedRef.current;
    observedRef.current = state;
    if (previous === state || suppressRef.current) return;
    if (applyingRef.current) {
      applyingRef.current = false;
      return;
    }

    if (gestureRef.current) return;
    const key = changedKey(previous, state);
    if (pendingRef.current && pendingRef.current.key !== key) finishGroup();
    pendingRef.current ??= { key, start: previous };
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(finishGroup, GROUP_DELAY_MS);
  }, [finishGroup, state]);

  const beginGesture = useCallback(() => {
    if (gestureRef.current) return;
    finishGroup();
    gestureRef.current = true;
    pendingRef.current = { key: undefined, start: currentRef.current };
  }, [finishGroup]);

  const endGesture = useCallback(() => {
    if (!gestureRef.current) return;
    gestureRef.current = false;
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    // React commits the final pointer update after the handler returns. Finish
    // on the next task so the gesture's last frame belongs to this undo step.
    timerRef.current = window.setTimeout(finishGroup, 0);
  }, [finishGroup]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        ownsTextEditingKeys(event.target) ||
        event.altKey ||
        event.isComposing ||
        event.repeat
      )
        return;
      const modifier = event.metaKey || event.ctrlKey;
      if (!modifier || event.key.toLowerCase() !== "z") return;
      event.preventDefault();

      if (event.shiftKey) {
        finishGroup();
        const next = futureRef.current.pop();
        if (!next) return;
        pastRef.current.push(currentRef.current);
        applyingRef.current = true;
        applyRef.current(next);
        return;
      }

      const pending = pendingRef.current;
      if (pending) {
        if (timerRef.current !== null) window.clearTimeout(timerRef.current);
        timerRef.current = null;
        pendingRef.current = null;
        futureRef.current.push(currentRef.current);
        applyingRef.current = true;
        applyRef.current(pending.start);
        return;
      }
      const previous = pastRef.current.pop();
      if (!previous) return;
      futureRef.current.push(currentRef.current);
      applyingRef.current = true;
      applyRef.current(previous);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [finishGroup]);

  return useMemo(
    () => ({ beginGesture, endGesture }),
    [beginGesture, endGesture],
  );
}
