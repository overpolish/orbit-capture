// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { defaultCameraOverlay } from "../recording-export-settings";
import { useExportWindowShortcuts } from "../use-export-window-shortcuts";
import { useRecordingPreviewPlayer } from "../use-recording-preview-player";
import { useRecordingTimelineThumbnails } from "../use-recording-timeline-thumbnails";

import { AudioVisualizer } from "./audio-visualizer";
import { BakedCameraPreviewViewport } from "./baked-camera-preview-viewport";
import { PreviewToolbar } from "./preview-toolbar";
import { RecordingPlaybackControls } from "./recording-playback-controls";
import { RecordingPreviewViewport } from "./recording-preview-viewport";
import { RecordingTrackLanes } from "./recording-track-lanes";
import { createPlayhead } from "./scrub-playhead";
import { useCameraOverlayHistory } from "./use-camera-overlay-history";

import type { ScrubPreviewProps } from "./scrub-preview";

const EMPTY_AUDIO_TRACKS: NonNullable<ScrubPreviewProps["audioTracks"]> = [];
const PREVIEW_PANE_GAP = 24;

/** Export playback whose decode, audio output and timeline are all owned by Rust. */
export function NativeRecordingPreview({
  artifactId,
  audioError,
  audioTrackVolumes = [],
  audioTracks = EMPTY_AUDIO_TRACKS,
  bakeCamera = false,
  cameraOverlay = defaultCameraOverlay(),
  cursorEffects = {
    bake: true,
    clickAnimation: true,
    motionBlur: true,
    sizePercent: 100,
    smoothMovement: true,
  },
  durationMs,
  enabledStreamIndices,
  enabledVideoTracks = [],
  inspector,
  isPreparingAudio = false,
  isPreparingPreview = false,
  onCameraOverlayChange,
  onEnabledTracksChange,
  onEnabledVideoTracksChange,
  onSelectedTrackChange,
  previewLayout,
  previewOutputDimensions,
  selectedTrack = null,
}: ScrubPreviewProps & { inspector?: ReactNode }) {
  const screenCanvasRef = useRef<HTMLCanvasElement>(null);
  const cameraCanvasRef = useRef<HTMLCanvasElement>(null);
  const totalDurationRef = useRef(durationMs);
  const [playhead] = useState(createPlayhead);
  const [isScrubbing, setIsScrubbing] = useState(false);
  const [zoomPercent, setZoomPercent] = useState(100);
  const selectedStreamIndices =
    enabledStreamIndices ?? audioTracks.map((track) => track.streamIndex);
  const enabledTracks = new Set(selectedStreamIndices);
  const selectedVideoTracks = new Set(enabledVideoTracks);
  const audioVolumeByStream = useMemo(
    () =>
      new Map(
        audioTrackVolumes.map(({ decibels, streamIndex }) => [
          streamIndex,
          decibels,
        ]),
      ),
    [audioTrackVolumes],
  );
  const player = useRecordingPreviewPlayer({
    artifactId,
    audioTrackVolumes,
    cameraCanvasRef,
    cursorEffects,
    enabledStreamIndices: selectedStreamIndices,
    isEnabled: previewLayout === undefined,
    onPosition: (positionMs) => {
      const total = totalDurationRef.current;
      playhead.publish(positionMs / 1_000, total > 0 ? positionMs / total : 0);
    },
    screenCanvasRef,
  });
  const timelineThumbnails = useRecordingTimelineThumbnails({
    artifactId,
    isEnabled: previewLayout === undefined,
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
  const visiblePaneEntries =
    layout?.panes
      .map((pane, index) => ({
        canvasRef: canvasRefs[index],
        pane,
        trackId: index === 0 ? ("primary" as const) : ("camera" as const),
      }))
      .filter(({ trackId }) => selectedVideoTracks.has(trackId)) ?? [];
  const visiblePreviewHeight = visiblePaneEntries.reduce(
    (height, { pane }) => Math.max(height, pane.height),
    0,
  );
  let visiblePreviewX = 0;
  const visibleLayout = layout
    ? {
        height: visiblePreviewHeight,
        panes: visiblePaneEntries.map(({ pane }) => {
          const visiblePane = {
            ...pane,
            x: visiblePreviewX,
            y: (visiblePreviewHeight - pane.height) / 2,
          };
          visiblePreviewX += pane.width + PREVIEW_PANE_GAP;
          return visiblePane;
        }),
        width: Math.max(0, visiblePreviewX - PREVIEW_PANE_GAP),
      }
    : null;
  const visibleCanvasRefs = visiblePaneEntries.map(
    ({ canvasRef }) => canvasRef,
  );
  const screenPane = layout?.panes[0];
  const cameraPane = layout?.panes[1];
  const canPreviewBakedCamera =
    bakeCamera &&
    selectedVideoTracks.has("primary") &&
    selectedVideoTracks.has("camera");
  const isPlaying = player.isPlaying;
  const pause = player.pause;
  const play = player.play;
  const togglePlayback = useCallback(() => {
    if (isPlaying) pause();
    else play();
  }, [isPlaying, pause, play]);

  useExportWindowShortcuts({
    onTogglePlayback: layout ? togglePlayback : undefined,
  });

  useEffect(() => {
    playhead.publish(0, 0);
  }, [artifactId, playhead]);

  const seek = (ratio: number, phase: "end" | "move" | "start") => {
    if (phase === "start") setIsScrubbing(true);
    if (phase === "end") setIsScrubbing(false);
    const positionMs = ratio * totalDurationMs;
    playhead.publish(positionMs / 1_000, ratio);
    player.seek(positionMs, phase);
  };

  return (
    <div className="flex min-h-0 grow flex-col">
      <div className="grid min-h-0 grow grid-cols-[clamp(240px,23vw,300px)_minmax(0,1fr)]">
        {inspector}
        <section className="flex min-h-0 min-w-0 flex-col bg-black/5 dark:bg-black/25">
          {visibleLayout && visibleLayout.panes.length > 0 ? (
            <PreviewToolbar
              badges={visibleLayout.panes.map((pane, index) => {
                const outputDimensions =
                  previewOutputDimensions?.[visiblePaneEntries[index].trackId];
                return {
                  height: outputDimensions?.height ?? pane.sourceHeight,
                  kind: pane.kind,
                  width: outputDimensions?.width ?? pane.sourceWidth,
                };
              })}
              onZoomChange={setZoomPercent}
              zoomPercent={zoomPercent}
            />
          ) : null}
          <div className="flex min-h-0 grow items-stretch justify-center">
            {!layout ? (
              <div className="flex grow items-center justify-center gap-3 text-xs text-muted">
                <CircularProgressBar
                  aria-label="Preparing recording preview"
                  isIndeterminate
                  size={32}
                  strokeWidth={10}
                />
                Preparing recording preview
              </div>
            ) : canPreviewBakedCamera && screenPane && cameraPane ? (
              <div className="flex min-h-0 min-w-0 grow flex-col">
                <BakedCameraPreviewViewport
                  cameraCanvasRef={cameraCanvasRef}
                  cameraPane={cameraPane}
                  controlsVisible={!isPlaying && !isScrubbing}
                  isBusy={
                    previewLayout === undefined &&
                    (player.isPreparing || isPreparingPreview)
                  }
                  onInteractionEnd={cameraHistory.endGesture}
                  onInteractionStart={cameraHistory.beginGesture}
                  onNeedFullResolution={player.requestFullResolution}
                  onSettingsChange={cameraHistory.change}
                  onZoomChange={setZoomPercent}
                  screenCanvasRef={screenCanvasRef}
                  screenPane={screenPane}
                  settings={cameraOverlay}
                  zoomPercent={zoomPercent}
                />
              </div>
            ) : visibleLayout && visibleLayout.panes.length > 0 ? (
              <div className="flex min-h-0 min-w-0 grow flex-col">
                <RecordingPreviewViewport
                  canvasRefs={visibleCanvasRefs}
                  isBusy={
                    previewLayout === undefined &&
                    (player.isPreparing || isPreparingPreview)
                  }
                  layout={visibleLayout}
                  onNeedFullResolution={player.requestFullResolution}
                  onZoomChange={setZoomPercent}
                  zoomPercent={zoomPercent}
                />
              </div>
            ) : (
              <AudioVisualizer
                audioTracks={audioTracks}
                enabledTracks={enabledTracks}
                playhead={playhead}
                volumes={audioVolumeByStream}
              />
            )}
          </div>

          {audioError ? (
            <p className="m-0 px-4 pb-2 text-xs text-error">{audioError}</p>
          ) : null}
          {player.error ? (
            <p className="m-0 px-4 pb-2 text-xs text-error">{player.error}</p>
          ) : null}

          {layout ? (
            <RecordingPlaybackControls
              durationMs={totalDurationMs}
              isPlaying={player.isPlaying}
              onPause={player.pause}
              onPlay={player.play}
              playhead={playhead}
            />
          ) : null}
        </section>
      </div>

      {layout ? (
        isPreparingAudio ? (
          <div className="flex h-24 shrink-0 items-center justify-center gap-2 border-t border-muted/15 text-xs text-muted">
            <CircularProgressBar
              aria-label="Preparing audio preview"
              isIndeterminate
              size={22}
              strokeWidth={8}
            />
            Preparing audio tracks
          </div>
        ) : (
          <RecordingTrackLanes
            audioTracks={audioTracks}
            durationMs={totalDurationMs}
            enabledTracks={enabledTracks}
            enabledVideoTracks={selectedVideoTracks}
            layout={layout}
            onEnabledTracksChange={(tracks) => {
              onEnabledTracksChange?.([...tracks]);
            }}
            onEnabledVideoTracksChange={(tracks) => {
              onEnabledVideoTracksChange?.([...tracks]);
            }}
            onSeek={seek}
            onSelectedTrackChange={(streamIndex) => {
              onSelectedTrackChange?.(streamIndex);
            }}
            playhead={playhead}
            selectedTrack={selectedTrack}
            thumbnails={timelineThumbnails}
            volumes={audioVolumeByStream}
          />
        )
      ) : null}
    </div>
  );
}
