// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useRef, useState } from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { defaultCameraOverlay } from "../recording-export-settings";
import { useRecordingPreviewPlayer } from "../use-recording-preview-player";

import { BakedCameraPreviewViewport } from "./baked-camera-preview-viewport";
import { CanvasPreviewViewport } from "./canvas-preview-viewport";
import { RecordingPlaybackControls } from "./recording-playback-controls";
import { ScrubAudioTracks } from "./scrub-audio-tracks";
import { createPlayhead } from "./scrub-playhead";
import { useCameraOverlayHistory } from "./use-camera-overlay-history";

import type { ScrubPreviewProps } from "./scrub-preview";

const EMPTY_AUDIO_TRACKS: NonNullable<ScrubPreviewProps["audioTracks"]> = [];

/** Export playback whose decode, audio output and timeline are all owned by Rust. */
export function NativeRecordingPreview({
  artifactId,
  audioError,
  audioTracks = EMPTY_AUDIO_TRACKS,
  bakeCamera = false,
  cameraOverlay = defaultCameraOverlay(),
  durationMs,
  enabledStreamIndices,
  isPreparingAudio = false,
  isPreparingPreview = false,
  onCameraOverlayChange,
  onEnabledTracksChange,
  previewLayout,
}: ScrubPreviewProps) {
  const screenCanvasRef = useRef<HTMLCanvasElement>(null);
  const cameraCanvasRef = useRef<HTMLCanvasElement>(null);
  const totalDurationRef = useRef(durationMs);
  const [playhead] = useState(createPlayhead);
  const selectedStreamIndices =
    enabledStreamIndices ?? audioTracks.map((track) => track.streamIndex);
  const enabledTracks = new Set(selectedStreamIndices);
  const player = useRecordingPreviewPlayer({
    artifactId,
    cameraCanvasRef,
    enabledStreamIndices: selectedStreamIndices,
    isEnabled: previewLayout === undefined,
    onPosition: (positionMs) => {
      const total = totalDurationRef.current;
      playhead.publish(positionMs / 1_000, total > 0 ? positionMs / total : 0);
    },
    screenCanvasRef,
  });
  const cameraHistory = useCameraOverlayHistory({
    enabled: bakeCamera,
    onChange: onCameraOverlayChange,
    resetKey: artifactId,
    settings: cameraOverlay,
  });
  const totalDurationMs = player.durationMs || durationMs;
  totalDurationRef.current = totalDurationMs;
  const layout = player.layout ?? previewLayout ?? null;
  const canvasRefs = [screenCanvasRef, cameraCanvasRef];
  const screenPane = layout?.panes[0];
  const cameraPane = layout?.panes[1];

  useEffect(() => {
    playhead.publish(0, 0);
  }, [artifactId, playhead]);

  return (
    <div className="flex flex-col gap-3">
      {!layout ? (
        <div className="flex h-[280px] items-center justify-center gap-3 text-xs text-muted">
          <CircularProgressBar
            aria-label="Preparing recording preview"
            isIndeterminate
            size={32}
            strokeWidth={10}
          />
          Preparing recording preview
        </div>
      ) : bakeCamera && screenPane && cameraPane ? (
        <div className="flex min-w-0 flex-col gap-2">
          <BakedCameraPreviewViewport
            cameraCanvasRef={cameraCanvasRef}
            cameraPane={cameraPane}
            isBusy={
              previewLayout === undefined &&
              (player.isPreparing || isPreparingPreview)
            }
            onInteractionEnd={cameraHistory.endGesture}
            onInteractionStart={cameraHistory.beginGesture}
            onNeedFullResolution={player.requestFullResolution}
            onSettingsChange={cameraHistory.change}
            screenCanvasRef={screenCanvasRef}
            screenPane={screenPane}
            settings={cameraOverlay}
          />
          <p className="m-0 text-center text-xxs text-muted tabular-nums">
            {screenPane.sourceWidth} &times; {screenPane.sourceHeight}
          </p>
        </div>
      ) : (
        <div
          className={
            layout.panes.length > 1
              ? "grid grid-cols-2 items-start gap-3"
              : "grid grid-cols-1 items-start gap-3"
          }
        >
          {layout.panes.map((pane, index) => (
            <div className="flex min-w-0 flex-col gap-2" key={pane.kind}>
              <CanvasPreviewViewport
                canvasRef={canvasRefs[index]}
                height={pane.height}
                isBusy={
                  previewLayout === undefined &&
                  (player.isPreparing || isPreparingPreview)
                }
                label={`${pane.kind === "camera" ? "Camera" : "Screen"} preview`}
                onNeedFullResolution={player.requestFullResolution}
                width={pane.width}
              />
              <p className="m-0 text-center text-xxs text-muted tabular-nums">
                {pane.sourceWidth} &times; {pane.sourceHeight}
              </p>
            </div>
          ))}
        </div>
      )}

      {layout ? (
        <RecordingPlaybackControls
          durationMs={totalDurationMs}
          isPlaying={player.isPlaying}
          onPause={player.pause}
          onPlay={player.play}
          onSeek={(ratio, phase) => {
            const positionMs = ratio * totalDurationMs;
            playhead.publish(positionMs / 1_000, ratio);
            player.seek(positionMs, phase);
          }}
          playhead={playhead}
        />
      ) : null}

      {audioError ? (
        <p className="m-0 text-xs text-error">{audioError}</p>
      ) : null}
      {player.error ? (
        <p className="m-0 text-xs text-error">{player.error}</p>
      ) : null}

      {isPreparingAudio ? (
        <div className="flex h-10 items-center justify-center gap-2 text-xs text-muted">
          <CircularProgressBar
            aria-label="Preparing audio preview"
            isIndeterminate
            size={22}
            strokeWidth={8}
          />
          Preparing audio tracks
        </div>
      ) : audioTracks.length > 0 ? (
        <ScrubAudioTracks
          audioTracks={audioTracks}
          enabledTracks={enabledTracks}
          onEnabledTracksChange={(tracks) => {
            onEnabledTracksChange?.([...tracks]);
          }}
          onSeek={(ratio, phase) => {
            const positionMs = ratio * totalDurationMs;
            playhead.publish(positionMs / 1_000, ratio);
            player.seek(positionMs, phase);
          }}
          playhead={playhead}
        />
      ) : null}
    </div>
  );
}
