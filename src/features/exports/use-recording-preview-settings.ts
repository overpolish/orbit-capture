// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Dispatch, RefObject, SetStateAction, useEffect, useRef } from "react";

import {
  selectRecordingPreviewAudio,
  setRecordingPreviewAudioVolumes,
  setRecordingPreviewComposition,
  setRecordingPreviewCursorEffects,
} from "./api";
import { RecordingOutputSettings } from "./screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
} from "./types";
import { usePreviewCapabilities } from "./use-preview-capabilities";

export function useRecordingPreviewSettings({
  audioTrackVolumes,
  bakeCamera,
  cameraOverlay,
  cursorEffects,
  enabledStreamIndices,
  isEnabled,
  recordingOutput,
  sessionIdRef,
  setError,
  startedRef,
}: {
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  cameraOverlay: CameraOverlaySettings;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  isEnabled: boolean;
  recordingOutput: RecordingOutputSettings;
  sessionIdRef: RefObject<number>;
  setError: Dispatch<SetStateAction<string | null>>;
  startedRef: RefObject<boolean>;
}) {
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  const selection = enabledStreamIndices.join("-");
  const volumes = audioTrackVolumes
    .map(
      ({ decibels, streamIndex }) =>
        `${streamIndex.toString()}:${decibels.toString()}`,
    )
    .join("-");
  const cursor = Object.values(cursorEffects).join("-");
  const composition = JSON.stringify({
    bakeCamera,
    cameraOverlay,
    recordingOutput,
  });
  const pendingCompositionRef = useRef({
    bakeCamera,
    cameraOverlay,
    recordingOutput,
  });
  pendingCompositionRef.current = {
    bakeCamera,
    cameraOverlay,
    recordingOutput,
  };
  useEffect(() => {
    if (!isEnabled || !startedRef.current || nativeSurface) return;
    void selectRecordingPreviewAudio(
      enabledStreamIndices,
      sessionIdRef.current,
    ).catch(setError);
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [isEnabled, selection]);
  useEffect(() => {
    if (!isEnabled || !startedRef.current) return;
    void setRecordingPreviewAudioVolumes(
      audioTrackVolumes,
      sessionIdRef.current,
    ).catch(setError);
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [isEnabled, volumes]);
  useEffect(() => {
    if (!isEnabled || !startedRef.current) return;
    void setRecordingPreviewCursorEffects(
      cursorEffects,
      sessionIdRef.current,
    ).catch(setError);
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [cursor, isEnabled]);
  useEffect(() => {
    // The native surface receives the composition inside every layout invoke,
    // atomically with the pane rects it belongs to and ordered by requestId.
    // Sending it here as well creates a second, unordered channel: a redraw
    // for this composition can land after a newer layout and stretch the
    // previous canvas into the new rect for a frame.
    if (!isEnabled || !startedRef.current || nativeSurface) return;
    // Inspector and OSC changes can arrive faster than the display. Forward
    // only the newest composition once per display tick so pointer input never
    // creates an IPC/render backlog behind the visible controls.
    const frame = requestAnimationFrame(() => {
      const pending = pendingCompositionRef.current;
      void setRecordingPreviewComposition({
        bakeCamera: pending.bakeCamera,
        cameraOverlay: pending.cameraOverlay,
        recordingOutput: pending.recordingOutput,
        sessionId: sessionIdRef.current,
      }).catch(setError);
    });
    return () => {
      cancelAnimationFrame(frame);
    };
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [composition, isEnabled]);
}
