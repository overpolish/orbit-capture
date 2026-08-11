// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { RefObject, useCallback, useEffect, useRef, useState } from "react";

import {
  pauseRecordingPreview,
  playRecordingPreview,
  RecordingPreviewPlayerEvent,
  requestRecordingPreviewFullResolution,
  seekRecordingPreview,
  selectRecordingPreviewAudio,
  setRecordingPreviewAudioVolumes,
  setRecordingPreviewCursorEffects,
  startRecordingPreviewPlayer,
  stopRecordingPreviewPlayer,
} from "./api";
import { ScrubPhase } from "./components/scrub-timeline";
import { AudioTrackVolume, CursorEffectSettings } from "./types";
import { useRecordingPreviewFrames } from "./use-recording-preview-frames";

let sessionSequence = 0;

export function useRecordingPreviewPlayer({
  artifactId,
  audioTrackVolumes,
  cameraCanvasRef,
  cursorEffects,
  enabledStreamIndices,
  isEnabled,
  onPosition,
  screenCanvasRef,
}: {
  artifactId: number;
  audioTrackVolumes: AudioTrackVolume[];
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  isEnabled: boolean;
  onPosition: (positionMs: number) => void;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
}) {
  const latestSeekRef = useRef<Promise<unknown>>(Promise.resolve());
  const pendingSeekRef = useRef<number | null>(null);
  const seekCompletionRef = useRef<(() => void) | null>(null);
  const seekInFlightRef = useRef(false);
  const activeSeekPositionRef = useRef<number | null>(null);
  const activeSeekRequestRef = useRef<number | null>(null);
  const settledSeekPositionRef = useRef<number | null>(null);
  const isPlayingRef = useRef(false);
  const resumeAfterSeekRef = useRef(false);
  const scrubFinishedRef = useRef(true);
  const onPositionRef = useRef(onPosition);
  const audioTrackVolumesRef = useRef(audioTrackVolumes);
  const cursorEffectsRef = useRef(cursorEffects);
  const enabledStreamIndicesRef = useRef(enabledStreamIndices);
  const durationRef = useRef(0);
  const positionRef = useRef(0);
  const seekRequestRef = useRef(0);
  const pendingSeekRequestRef = useRef(0);
  const desiredSeekRequestRef = useRef(0);
  const sessionIdRef = useRef(0);
  const sendNextSeekRef = useRef<() => void>(() => undefined);
  const startedRef = useRef(false);
  const [durationMs, setDurationMs] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const frames = useRecordingPreviewFrames({
    cameraCanvasRef,
    onError: setError,
    screenCanvasRef,
  });
  const selectionSignature = enabledStreamIndices.join("-");
  const volumeSignature = audioTrackVolumes
    .map(
      (volume) =>
        `${volume.streamIndex.toString()}:${volume.decibels.toString()}`,
    )
    .join("-");
  const cursorSignature = Object.values(cursorEffects).join("-");
  onPositionRef.current = onPosition;
  audioTrackVolumesRef.current = audioTrackVolumes;
  cursorEffectsRef.current = cursorEffects;
  enabledStreamIndicesRef.current = enabledStreamIndices;

  const updatePlaying = (playing: boolean) => {
    isPlayingRef.current = playing;
    setIsPlaying(playing);
  };

  const resumeAfterSettledSeek = () => {
    if (
      seekInFlightRef.current ||
      pendingSeekRef.current !== null ||
      !scrubFinishedRef.current ||
      !resumeAfterSeekRef.current
    )
      return;
    resumeAfterSeekRef.current = false;
    void playRecordingPreview(sessionIdRef.current).catch((cause: unknown) => {
      updatePlaying(false);
      setError(String(cause));
    });
  };

  const finishSeek = (
    canResume = true,
    requestId = activeSeekRequestRef.current,
  ) => {
    if (!seekInFlightRef.current) return;
    if (
      requestId !== null &&
      activeSeekRequestRef.current !== null &&
      requestId !== activeSeekRequestRef.current
    )
      return;
    seekInFlightRef.current = false;
    if (canResume)
      settledSeekPositionRef.current = activeSeekPositionRef.current;
    else resumeAfterSeekRef.current = false;
    activeSeekPositionRef.current = null;
    activeSeekRequestRef.current = null;
    seekCompletionRef.current?.();
    seekCompletionRef.current = null;
    if (pendingSeekRef.current !== null) sendNextSeekRef.current();
    else resumeAfterSettledSeek();
  };

  sendNextSeekRef.current = () => {
    if (!isEnabled || seekInFlightRef.current) return;
    const positionMs = pendingSeekRef.current;
    if (positionMs === null) return;
    pendingSeekRef.current = null;
    seekInFlightRef.current = true;
    activeSeekPositionRef.current = positionMs;
    const requestId = pendingSeekRequestRef.current;
    activeSeekRequestRef.current = requestId;
    latestSeekRef.current = new Promise<void>((resolve) => {
      seekCompletionRef.current = resolve;
    });
    void seekRecordingPreview(
      positionMs,
      requestId,
      sessionIdRef.current,
    ).catch((cause: unknown) => {
      setError(String(cause));
      finishSeek(false);
    });
  };

  useEffect(() => {
    if (!isEnabled) return;
    let disposed = false;
    const sessionId = Date.now() * 1_000 + (++sessionSequence % 1_000);
    sessionIdRef.current = sessionId;
    seekRequestRef.current = 0;
    pendingSeekRequestRef.current = 0;
    desiredSeekRequestRef.current = 0;
    frames.begin();
    const frameChannel = new Channel<ArrayBuffer>();
    frameChannel.onmessage = (frame) => {
      if (!disposed) frames.receive(frame);
    };
    const eventChannel = new Channel<RecordingPreviewPlayerEvent>();
    eventChannel.onmessage = (event) => {
      if (disposed) return;
      if (event.event === "error") {
        setError(event.data.message);
        frames.setIsPreparing(false);
        finishSeek(false);
      } else if (event.event === "ended") {
        updatePlaying(false);
        positionRef.current = durationRef.current;
        onPositionRef.current(durationRef.current);
        void pauseRecordingPreview(sessionIdRef.current).catch(() => undefined);
      } else {
        if (event.event === "ready") {
          if (event.data.requestId !== activeSeekRequestRef.current) return;
          if (event.data.requestId >= desiredSeekRequestRef.current) {
            positionRef.current = event.data.positionMs;
            onPositionRef.current(event.data.positionMs);
          }
          finishSeek(true, event.data.requestId);
          return;
        }
        if (
          seekInFlightRef.current ||
          pendingSeekRef.current !== null ||
          !scrubFinishedRef.current
        )
          return;
        positionRef.current = event.data.positionMs;
        onPositionRef.current(event.data.positionMs);
        if (event.event === "playing") {
          settledSeekPositionRef.current = null;
          updatePlaying(true);
        }
        if (event.event === "paused") updatePlaying(false);
      }
    };
    void startRecordingPreviewPlayer({
      artifactId,
      audioTrackVolumes,
      cursorEffects,
      enabledStreamIndices,
      eventChannel,
      frameChannel,
      sessionId,
    })
      .then((info) => {
        if (disposed) return;
        frames.setLayout(info.layout);
        durationRef.current = info.durationMs;
        setDurationMs(info.durationMs);
        startedRef.current = true;
        // The export window resets its controls for a new artifact while the
        // native player is starting. Those updates can land before the player
        // exists, so apply the latest values once this session is ready too.
        void Promise.all([
          selectRecordingPreviewAudio(
            enabledStreamIndicesRef.current,
            sessionId,
          ),
          setRecordingPreviewAudioVolumes(
            audioTrackVolumesRef.current,
            sessionId,
          ),
          setRecordingPreviewCursorEffects(cursorEffectsRef.current, sessionId),
        ])
          .then(() => {
            // The opening still is deliberately small so the window appears
            // quickly. Replace it once with source-resolution pixels instead
            // of waiting for the first zoom or pan to make it crisp.
            if (!disposed && info.layout.panes.length > 0)
              return requestRecordingPreviewFullResolution(sessionId);
          })
          .catch((cause: unknown) => {
            if (!disposed) setError(String(cause));
          });
      })
      .catch((cause: unknown) => {
        if (!disposed) {
          setError(String(cause));
          frames.setIsPreparing(false);
        }
      });
    return () => {
      disposed = true;
      startedRef.current = false;
      frames.reset();
      pendingSeekRef.current = null;
      activeSeekPositionRef.current = null;
      activeSeekRequestRef.current = null;
      settledSeekPositionRef.current = null;
      resumeAfterSeekRef.current = false;
      scrubFinishedRef.current = true;
      finishSeek(false);
      void stopRecordingPreviewPlayer(sessionId).catch(() => undefined);
    };
    // The initial selection belongs to player creation; later changes use the effect below.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [artifactId, isEnabled]);

  useEffect(() => {
    if (!isEnabled) return;
    if (!startedRef.current) return;
    void selectRecordingPreviewAudio(
      enabledStreamIndices,
      sessionIdRef.current,
    ).catch(setError);
    // The signature prevents a freshly allocated but identical selection from restarting playback.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [isEnabled, selectionSignature]);

  useEffect(() => {
    if (!isEnabled || !startedRef.current) return;
    void setRecordingPreviewAudioVolumes(
      audioTrackVolumes,
      sessionIdRef.current,
    ).catch(setError);
    // The signature keeps object identity changes from sending duplicate updates.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [isEnabled, volumeSignature]);

  useEffect(() => {
    if (!isEnabled || !startedRef.current) return;
    void setRecordingPreviewCursorEffects(
      cursorEffects,
      sessionIdRef.current,
    ).catch(setError);
    // The signature avoids duplicate native redraws from object identity changes.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [cursorSignature, isEnabled]);

  const play = useCallback(() => {
    if (!isEnabled) return;
    resumeAfterSeekRef.current = false;
    scrubFinishedRef.current = true;
    setError(null);
    updatePlaying(true);
    void (async () => {
      try {
        if (positionRef.current >= durationRef.current) {
          positionRef.current = 0;
          const requestId = ++seekRequestRef.current;
          desiredSeekRequestRef.current = requestId;
          await seekRecordingPreview(0, requestId, sessionIdRef.current);
        }
        while (seekInFlightRef.current || pendingSeekRef.current !== null) {
          await latestSeekRef.current;
        }
        await playRecordingPreview(sessionIdRef.current);
      } catch (cause) {
        updatePlaying(false);
        setError(String(cause));
      }
    })();
  }, [isEnabled]);
  const pause = useCallback(() => {
    if (!isEnabled) return;
    resumeAfterSeekRef.current = false;
    scrubFinishedRef.current = true;
    updatePlaying(false);
    void (async () => {
      try {
        while (seekInFlightRef.current || pendingSeekRef.current !== null) {
          await latestSeekRef.current;
        }
        await pauseRecordingPreview(sessionIdRef.current);
      } catch (cause) {
        setError(String(cause));
      }
    })();
  }, [isEnabled]);
  const requestFullResolution = useCallback(() => {
    if (!isEnabled || isPlayingRef.current) return;
    void requestRecordingPreviewFullResolution(sessionIdRef.current).catch(
      (cause: unknown) => {
        setError(String(cause));
      },
    );
  }, [isEnabled]);
  const seek = (positionMs: number, phase: ScrubPhase) => {
    if (!isEnabled) return;
    const normalized = Math.max(0, Math.round(positionMs));
    if (phase === "start") {
      resumeAfterSeekRef.current = isPlayingRef.current;
      scrubFinishedRef.current = false;
    } else if (phase === "end") {
      scrubFinishedRef.current = true;
    }
    positionRef.current = normalized;
    updatePlaying(false);
    if (
      normalized !== activeSeekPositionRef.current &&
      normalized !== pendingSeekRef.current &&
      normalized !== settledSeekPositionRef.current
    ) {
      pendingSeekRef.current = normalized;
      const requestId = ++seekRequestRef.current;
      pendingSeekRequestRef.current = requestId;
      desiredSeekRequestRef.current = requestId;
      sendNextSeekRef.current();
    } else if (phase === "end") {
      resumeAfterSettledSeek();
    }
  };

  return {
    durationMs,
    error,
    isPlaying,
    isPreparing: frames.isPreparing,
    layout: frames.layout,
    pause,
    play,
    requestFullResolution,
    seek,
  };
}
