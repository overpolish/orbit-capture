// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { RefObject, useCallback, useEffect, useRef, useState } from "react";

import {
  pauseRecordingPreview,
  playRecordingPreview,
  RecordingPreviewPlayerEvent,
  seekRecordingPreview,
  selectRecordingPreviewAudio,
  setRecordingPreviewAudioVolumes,
  setRecordingPreviewComposition,
  setRecordingPreviewCursorEffects,
  setRecordingPreviewZoom,
  startRecordingPreviewPlayer,
  stopRecordingPreviewPlayer,
} from "./api";
import { ScrubPhase } from "./components/scrub-timeline";
import { AudioTrackVolume, CursorEffectSettings } from "./types";
import { useRecordingPreviewFrames } from "./use-recording-preview-frames";
import { useRecordingPreviewSettings } from "./use-recording-preview-settings";
import {
  type RecordingSelectionGestureEvent,
  useRecordingPreviewSurface,
} from "./use-recording-preview-surface";

let sessionSequence = 0;

const settingsKey = (settings: {
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  cameraOverlay: import("./types").CameraOverlaySettings;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  recordingOutput: import("./screenshot-output").RecordingOutputSettings;
}) => JSON.stringify(settings);

export function useRecordingPreviewPlayer({
  artifactId,
  audioTrackVolumes,
  bakeCamera,
  cameraCanvasRef,
  cameraOverlay,
  cursorCanvasRef,
  cursorEffects,
  enabledStreamIndices,
  isEditorSuspended,
  isEnabled,
  nativeEditorOwnsLayout,
  nativeLayoutHasPanes,
  nativeLayoutKey,
  onPosition,
  onSelectionChange,
  onSelectionGesture,
  onZoomChange,
  recordingOutput,
  screenCanvasRef,
  selection,
  selectionTargets,
  zoomPercent,
}: {
  artifactId: number;
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cameraOverlay: import("./types").CameraOverlaySettings;
  cursorCanvasRef: RefObject<HTMLCanvasElement | null>;
  cursorEffects: CursorEffectSettings;
  enabledStreamIndices: number[];
  isEditorSuspended: boolean;
  isEnabled: boolean;
  nativeEditorOwnsLayout: boolean;
  nativeLayoutHasPanes: boolean;
  nativeLayoutKey: string;
  onPosition: (positionMs: number) => void;
  recordingOutput: import("./screenshot-output").RecordingOutputSettings;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  onSelectionChange?: (paneIndex: number | null) => void;
  onSelectionGesture?: (event: RecordingSelectionGestureEvent) => void;
  onZoomChange?: (zoomPercent: number) => void;
  selection?: {
    paneIndex: number;
    radiusPercent: number;
    rect: { height: number; width: number; x: number; y: number };
    layerId?: number;
  } | null;
  selectionTargets?:
    | {
        paneIndex: number;
        radiusPercent: number;
        rect: { height: number; width: number; x: number; y: number };
        layerId?: number;
      }[]
    | null;
  zoomPercent?: number;
}) {
  const isPlayingRef = useRef(false);
  const wantsPlaybackRef = useRef(false);
  const resumeAfterSeekRef = useRef(false);
  const scrubFinishedRef = useRef(true);
  const onPositionRef = useRef(onPosition);
  const audioTrackVolumesRef = useRef(audioTrackVolumes);
  const cursorEffectsRef = useRef(cursorEffects);
  const compositionRef = useRef({ bakeCamera, cameraOverlay, recordingOutput });
  const enabledStreamIndicesRef = useRef(enabledStreamIndices);
  const durationRef = useRef(0);
  const positionRef = useRef(0);
  const seekRequestRef = useRef(0);
  const lastSentSeekRef = useRef<number | null>(null);
  const pendingScrubFrameRef = useRef<number | null>(null);
  const pendingScrubPositionRef = useRef<number | null>(null);
  const pendingResumeRequestRef = useRef<number | null>(null);
  const settleRequestRef = useRef<number | null>(null);
  const sessionIdRef = useRef(0);
  const startedRef = useRef(false);
  const [durationMs, setDurationMs] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const frames = useRecordingPreviewFrames({
    cameraCanvasRef,
    cursorCanvasRef,
    onError: setError,
    screenCanvasRef,
  });
  onPositionRef.current = onPosition;
  audioTrackVolumesRef.current = audioTrackVolumes;
  cursorEffectsRef.current = cursorEffects;
  compositionRef.current = { bakeCamera, cameraOverlay, recordingOutput };
  enabledStreamIndicesRef.current = enabledStreamIndices;

  useRecordingPreviewSettings({
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
  });

  useRecordingPreviewSurface({
    bakeCamera,
    cameraCanvasRef,
    cameraOverlay,
    isEditorSuspended,
    isEnabled,
    isPlaying,
    nativeEditorOwnsLayout,
    nativeLayoutHasPanes,
    nativeLayoutKey,
    onError: setError,
    onSelectionChange,
    onSelectionGesture,
    onZoomChange,
    recordingOutput,
    screenCanvasRef,
    selection,
    selectionTargets,
    sessionIdRef,
    startedRef,
    zoomPercent,
  });

  const updatePlaying = (playing: boolean) => {
    isPlayingRef.current = playing;
    setIsPlaying(playing);
  };

  useEffect(() => {
    if (!isEnabled) return;
    let disposed = false;
    const initialSettingsKey = settingsKey({
      audioTrackVolumes,
      bakeCamera,
      cameraOverlay,
      cursorEffects,
      enabledStreamIndices,
      recordingOutput,
    });
    const sessionId = Date.now() * 1_000 + (++sessionSequence % 1_000);
    sessionIdRef.current = sessionId;
    seekRequestRef.current = 0;
    lastSentSeekRef.current = null;
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
        return;
      }
      if (event.event === "ended") {
        wantsPlaybackRef.current = false;
        updatePlaying(false);
        positionRef.current = durationRef.current;
        onPositionRef.current(durationRef.current);
        void pauseRecordingPreview(sessionIdRef.current).catch(() => undefined);
        return;
      }
      if (event.event === "ready") {
        if (event.data.requestId < seekRequestRef.current) return;
        frames.setIsPreparing(false);
        positionRef.current = event.data.positionMs;
        onPositionRef.current(event.data.positionMs);
        if (pendingResumeRequestRef.current === event.data.requestId) {
          pendingResumeRequestRef.current = null;
          settleRequestRef.current = null;
          resumeAfterSeekRef.current = false;
          scrubFinishedRef.current = true;
          wantsPlaybackRef.current = true;
          void playRecordingPreview(sessionIdRef.current).catch(
            (cause: unknown) => {
              wantsPlaybackRef.current = false;
              updatePlaying(false);
              setError(String(cause));
            },
          );
        } else if (settleRequestRef.current === event.data.requestId) {
          settleRequestRef.current = null;
          scrubFinishedRef.current = true;
        }
        return;
      }
      // The playhead is frontend-driven while a scrub is in progress; stale
      // worker positions must not yank it backwards.
      if (!scrubFinishedRef.current) return;
      if (event.event === "position" && !isPlayingRef.current) return;
      positionRef.current = event.data.positionMs;
      onPositionRef.current(event.data.positionMs);
      if (event.event === "playing") {
        // Starting audio has a short native prebuffer. If Pause won during
        // that interval, its cancelled worker can still report that startup
        // reached Playing; the latest UI intent remains authoritative.
        if (wantsPlaybackRef.current) updatePlaying(true);
      }
      if (event.event === "paused" && !wantsPlaybackRef.current)
        updatePlaying(false);
    };
    void startRecordingPreviewPlayer({
      artifactId,
      audioTrackVolumes,
      bakeCamera,
      cameraOverlay,
      cursorEffects,
      enabledStreamIndices,
      eventChannel,
      frameChannel,
      recordingOutput,
      sessionId,
    })
      .then((info) => {
        if (disposed) return;
        frames.setLayout(info.layout);
        durationRef.current = info.durationMs;
        setDurationMs(info.durationMs);
        startedRef.current = true;
        if (
          nativeEditorOwnsLayout &&
          !isEditorSuspended &&
          zoomPercent !== undefined
        ) {
          void setRecordingPreviewZoom(sessionId, zoomPercent).catch(
            (cause: unknown) => {
              if (!disposed) setError(String(cause));
            },
          );
        }
        const latestSettingsKey = settingsKey({
          audioTrackVolumes: audioTrackVolumesRef.current,
          bakeCamera: compositionRef.current.bakeCamera,
          cameraOverlay: compositionRef.current.cameraOverlay,
          cursorEffects: cursorEffectsRef.current,
          enabledStreamIndices: enabledStreamIndicesRef.current,
          recordingOutput: compositionRef.current.recordingOutput,
        });
        // Startup already installed the exact settings passed above. Repeating
        // them restarts a paused native decoder for cursor and composition,
        // making first-open review needlessly decode its first frame three
        // times. Only catch up when controls genuinely changed while startup
        // was in flight.
        if (latestSettingsKey === initialSettingsKey) return;
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
          setRecordingPreviewComposition({
            bakeCamera: compositionRef.current.bakeCamera,
            cameraOverlay: compositionRef.current.cameraOverlay,
            recordingOutput: compositionRef.current.recordingOutput,
            sessionId,
          }),
        ]).catch((cause: unknown) => {
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
      lastSentSeekRef.current = null;
      pendingScrubPositionRef.current = null;
      if (pendingScrubFrameRef.current !== null) {
        cancelAnimationFrame(pendingScrubFrameRef.current);
        pendingScrubFrameRef.current = null;
      }
      pendingResumeRequestRef.current = null;
      settleRequestRef.current = null;
      resumeAfterSeekRef.current = false;
      wantsPlaybackRef.current = false;
      scrubFinishedRef.current = true;
      void stopRecordingPreviewPlayer(sessionId).catch(() => undefined);
    };
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [artifactId, isEnabled]);

  const play = useCallback(() => {
    if (!isEnabled) return;
    resumeAfterSeekRef.current = false;
    wantsPlaybackRef.current = true;
    scrubFinishedRef.current = true;
    lastSentSeekRef.current = null;
    setError(null);
    updatePlaying(true);
    void (async () => {
      try {
        if (positionRef.current >= durationRef.current) {
          positionRef.current = 0;
          await seekRecordingPreview({
            positionMs: 0,
            requestId: ++seekRequestRef.current,
            sessionId: sessionIdRef.current,
          });
        }
        await playRecordingPreview(sessionIdRef.current);
      } catch (cause) {
        wantsPlaybackRef.current = false;
        updatePlaying(false);
        setError(String(cause));
      }
    })();
  }, [isEnabled]);
  const pause = useCallback(() => {
    if (!isEnabled) return;
    resumeAfterSeekRef.current = false;
    wantsPlaybackRef.current = false;
    pendingResumeRequestRef.current = null;
    settleRequestRef.current = null;
    scrubFinishedRef.current = true;
    lastSentSeekRef.current = null;
    updatePlaying(false);
    void pauseRecordingPreview(sessionIdRef.current).catch((cause: unknown) => {
      setError(String(cause));
    });
  }, [isEnabled]);
  const seek = (positionMs: number, phase: ScrubPhase) => {
    if (!isEnabled) return;
    const normalized = Math.max(0, Math.round(positionMs));
    if (phase === "start") {
      resumeAfterSeekRef.current = isPlayingRef.current;
      wantsPlaybackRef.current = false;
      scrubFinishedRef.current = false;
    }
    positionRef.current = normalized;
    // A seek that will resume playback keeps the UI in its playing state:
    // flipping the button to "play" and revealing the paused chrome for the
    // split second between click, still, and resumed playback reads as a
    // stutter. Internally the backend still pauses and resumes; only the
    // presented state holds steady. A seek from a genuine pause (or a resume
    // that fails - its catch below drops the state) behaves as before.
    if (!resumeAfterSeekRef.current) updatePlaying(false);
    const send = (nextPosition: number, nextPhase: ScrubPhase) => {
      // Start/end also carry native OSC visibility, so only movement samples
      // at the same playhead position are redundant.
      if (nextPosition === lastSentSeekRef.current && nextPhase === "move")
        return;
      lastSentSeekRef.current = nextPosition;
      const requestId = ++seekRequestRef.current;
      if (nextPhase === "end") {
        settleRequestRef.current = requestId;
        if (resumeAfterSeekRef.current)
          pendingResumeRequestRef.current = requestId;
      }
      void seekRecordingPreview({
        positionMs: nextPosition,
        requestId,
        rough: nextPhase !== "end",
        selectionVisible:
          nextPhase === "start"
            ? false
            : nextPhase === "end"
              ? true
              : undefined,
        sessionId: sessionIdRef.current,
      }).catch((cause: unknown) => {
        if (settleRequestRef.current === requestId) {
          settleRequestRef.current = null;
          scrubFinishedRef.current = true;
        }
        setError(String(cause));
      });
    };
    // Raw pointer events can arrive substantially faster than either the
    // display or decoder. Send only the newest position once per display tick
    // so the Tauri command queue cannot build a stale seek backlog.
    if (phase === "move") {
      pendingScrubPositionRef.current = normalized;
      if (pendingScrubFrameRef.current === null) {
        pendingScrubFrameRef.current = requestAnimationFrame(() => {
          pendingScrubFrameRef.current = null;
          const pending = pendingScrubPositionRef.current;
          pendingScrubPositionRef.current = null;
          if (pending !== null) send(pending, "move");
        });
      }
    } else {
      if (pendingScrubFrameRef.current !== null) {
        cancelAnimationFrame(pendingScrubFrameRef.current);
        pendingScrubFrameRef.current = null;
      }
      pendingScrubPositionRef.current = null;
      send(normalized, phase);
    }
    if (phase === "end") {
      // The ready event settles the exact frame and resumes playback if the
      // gesture began while playing. Starting playback here can race the still
      // decoder and replace the release frame.
      scrubFinishedRef.current = false;
    }
  };

  const getPositionMs = useCallback(() => positionRef.current, []);

  return {
    durationMs,
    error,
    getPositionMs,
    isPlaying,
    isPreparing: frames.isPreparing,
    layout: frames.layout,
    pause,
    play,
    seek,
  };
}
