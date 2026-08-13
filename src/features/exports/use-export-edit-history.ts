// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useRef } from "react";

import { ownsTextEditingKeys } from "./keyboard-target";
import {
  RecordingOutputSettings,
  ScreenshotOutputSettings,
} from "./screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
  RecordingVideoTrackId,
} from "./types";

const HISTORY_LIMIT = 100;
const GROUP_DELAY_MS = 300;

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
  screenshotOutput: ScreenshotOutputSettings;
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
  resetKey: number | undefined;
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
  const suppressRef = useRef(true);
  applyRef.current = apply;
  currentRef.current = state;

  const finishGroup = useCallback(() => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = null;
    const pending = pendingRef.current;
    pendingRef.current = null;
    if (!pending) return;
    pastRef.current.push(pending.start);
    if (pastRef.current.length > HISTORY_LIMIT) pastRef.current.shift();
    futureRef.current = [];
  }, []);

  useEffect(() => {
    suppressRef.current = true;
    pastRef.current = [];
    futureRef.current = [];
    pendingRef.current = null;
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

    const key = changedKey(previous, state);
    if (pendingRef.current && pendingRef.current.key !== key) finishGroup();
    pendingRef.current ??= { key, start: previous };
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(finishGroup, GROUP_DELAY_MS);
  }, [finishGroup, state]);

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
}
