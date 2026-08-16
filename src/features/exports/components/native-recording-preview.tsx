// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  ReactNode,
  MouseEvent as ReactMouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import {
  copyRecordingPreviewFrameToClipboard,
  copyRecordingPreviewSourceFrame,
} from "../api";
import { uncroppedCameraPreviewOverlay } from "../camera-overlay-geometry";
import { defaultCameraOverlay } from "../recording-export-settings";
import {
  RecordingOutputSettings,
  defaultRecordingOutput,
  recordingVideoTrackOrder,
  uncroppedScreenshotPreviewOutput,
} from "../screenshot-output";
import { RecordingTrackId, RecordingVideoTrackId } from "../types";
import { useExportWindowShortcuts } from "../use-export-window-shortcuts";
import { useRecordingPreviewPlayer } from "../use-recording-preview-player";
import { useRecordingTimelineThumbnails } from "../use-recording-timeline-thumbnails";

import { AudioVisualizer } from "./audio-visualizer";
import { BakedCameraPreviewViewport } from "./baked-camera-preview-viewport";
import { PreviewToolbar } from "./preview-toolbar";
import {
  RecordingCanvasTools,
  RecordingCanvasTool,
} from "./recording-crop-toggle";
import { RecordingOutputPreviewViewport } from "./recording-output-preview-viewport";
import { RecordingPlaybackControls } from "./recording-playback-controls";
import { RecordingPreviewViewport } from "./recording-preview-viewport";
import { RecordingTrackLanes } from "./recording-track-lanes";
import {
  LayerContextMenu,
  LayerContextMenuState,
} from "./screenshot-layer-context-menu";
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
    clipAtVideoEdge: false,
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
  onRecordingOutputChange,
  onSelectedTrackChange,
  onVideoTrackOrderChange,
  previewLayout,
  previewOutputDimensions,
  previewSourceDimensions,
  recordingOutput,
  selectedTrack = null,
}: ScrubPreviewProps & { inspector?: ReactNode }) {
  const screenCanvasRef = useRef<HTMLCanvasElement>(null);
  const cameraCanvasRef = useRef<HTMLCanvasElement>(null);
  const cursorCanvasRef = useRef<HTMLCanvasElement>(null);
  const totalDurationRef = useRef(durationMs);
  const [playhead] = useState(createPlayhead);
  const [isScrubbing, setIsScrubbing] = useState(false);
  const [zoomPercent, setZoomPercent] = useState(100);
  const [canvasTool, setCanvasTool] = useState<RecordingCanvasTool>("select");
  // A canvas resize runs at pointer rate; committing every move to the export
  // window's state re-renders the inspector, lanes and timeline and starves
  // the native pane's layout loop. The gesture renders from this draft and
  // commits once on release, exactly like the screenshot editor.
  const [canvasResizeDraft, setCanvasResizeDraft] =
    useState<RecordingOutputSettings | null>(null);
  const [layerContextMenu, setLayerContextMenu] =
    useState<LayerContextMenuState<RecordingVideoTrackId> | null>(null);
  const [copyState, setCopyState] = useState<"copying" | "done" | "idle">(
    "idle",
  );
  const [copyError, setCopyError] = useState<string | null>(null);
  // Everything derived below feeds memoized children. A canvas-resize gesture
  // re-renders this component at pointer rate, so a derived array or Set rebuilt
  // per render would defeat the memo of every subtree it reaches.
  const selectedStreamIndices = useMemo(
    () => enabledStreamIndices ?? audioTracks.map((track) => track.streamIndex),
    [audioTracks, enabledStreamIndices],
  );
  const enabledTracks = useMemo(
    () => new Set(selectedStreamIndices),
    [selectedStreamIndices],
  );
  const selectedVideoTracks = useMemo(
    () => new Set(enabledVideoTracks),
    [enabledVideoTracks],
  );
  const activeVideoTrack =
    selectedTrack === "primary" || selectedTrack === "camera"
      ? selectedTrack
      : null;
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
  const activeRecordingOutput =
    (canvasTool === "canvas" ? canvasResizeDraft : null) ?? recordingOutput;
  const effectiveRecordingOutput =
    activeRecordingOutput ??
    defaultRecordingOutput({
      camera: previewOutputDimensions?.camera,
      primary: previewOutputDimensions?.primary ?? { height: 64, width: 64 },
    });
  // Only the layer order matters here, and it is the one part of the output a
  // resize never touches; keying on it holds the array identity across a drag.
  const videoTrackOrder = useMemo(
    () => recordingVideoTrackOrder(effectiveRecordingOutput),
    // eslint-disable-next-line @eslint-react/exhaustive-deps
    [effectiveRecordingOutput.cameraOnTop],
  );
  const videoTrackOrderList = useMemo(
    () => [...videoTrackOrder],
    [videoTrackOrder],
  );
  const cropSource =
    activeVideoTrack === "primary"
      ? previewSourceDimensions.primary
      : previewSourceDimensions.camera;
  const previewRecordingOutput = useMemo(() => {
    if (canvasTool !== "crop" || !activeVideoTrack || !cropSource)
      return effectiveRecordingOutput;
    if (bakeCamera && activeVideoTrack === "camera")
      return effectiveRecordingOutput;
    return {
      ...effectiveRecordingOutput,
      [activeVideoTrack]: uncroppedScreenshotPreviewOutput(
        cropSource,
        effectiveRecordingOutput[activeVideoTrack],
      ),
    };
  }, [
    activeVideoTrack,
    bakeCamera,
    canvasTool,
    cropSource,
    effectiveRecordingOutput,
  ]);
  const previewCameraOverlay = useMemo(() => {
    if (
      canvasTool !== "crop" ||
      activeVideoTrack !== "camera" ||
      !bakeCamera ||
      !previewSourceDimensions.camera
    )
      return cameraOverlay;
    const output = effectiveRecordingOutput.primary;
    return uncroppedCameraPreviewOverlay(
      {
        height: output.height,
        kind: "screen",
        sourceHeight: output.height,
        sourceWidth: output.width,
        width: output.width,
        x: 0,
        y: 0,
      },
      {
        height: previewSourceDimensions.camera.height,
        kind: "camera",
        sourceHeight: previewSourceDimensions.camera.height,
        sourceWidth: previewSourceDimensions.camera.width,
        width: previewSourceDimensions.camera.width,
        x: 0,
        y: 0,
      },
      cameraOverlay,
    );
  }, [
    activeVideoTrack,
    bakeCamera,
    cameraOverlay,
    canvasTool,
    effectiveRecordingOutput.primary,
    previewSourceDimensions.camera,
  ]);
  const player = useRecordingPreviewPlayer({
    artifactId,
    audioTrackVolumes,
    bakeCamera,
    cameraCanvasRef,
    cameraOverlay: previewCameraOverlay,
    cursorCanvasRef,
    cursorEffects,
    enabledStreamIndices: selectedStreamIndices,
    isEnabled: previewLayout === undefined,
    onPosition: (positionMs) => {
      const total = totalDurationRef.current;
      playhead.publish(positionMs / 1_000, total > 0 ? positionMs / total : 0);
    },
    recordingOutput: previewRecordingOutput,
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
  const canvasRefs = useMemo(
    () => [screenCanvasRef, cameraCanvasRef],
    [cameraCanvasRef, screenCanvasRef],
  );
  const visiblePaneEntries = useMemo(
    () =>
      layout?.panes
        .map((pane, index) => ({
          canvasRef: canvasRefs[index],
          pane,
          trackId: index === 0 ? ("primary" as const) : ("camera" as const),
        }))
        .filter(({ trackId }) => selectedVideoTracks.has(trackId))
        .sort(
          (left, right) =>
            videoTrackOrder.indexOf(left.trackId) -
            videoTrackOrder.indexOf(right.trackId),
        ) ?? [],
    [canvasRefs, layout, selectedVideoTracks, videoTrackOrder],
  );
  const visibleLayout = useMemo(() => {
    if (!layout) return null;
    const height = visiblePaneEntries.reduce(
      (maximum, { pane }) => Math.max(maximum, pane.height),
      0,
    );
    let x = 0;
    const panes = visiblePaneEntries.map(({ pane }) => {
      const visiblePane = { ...pane, x, y: (height - pane.height) / 2 };
      x += pane.width + PREVIEW_PANE_GAP;
      return visiblePane;
    });
    return { height, panes, width: Math.max(0, x - PREVIEW_PANE_GAP) };
  }, [layout, visiblePaneEntries]);
  const visibleCanvasRefs = useMemo(
    () => visiblePaneEntries.map(({ canvasRef }) => canvasRef),
    [visiblePaneEntries],
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
  const canEditActiveTrack =
    activeVideoTrack !== null && selectedVideoTracks.has(activeVideoTrack);
  const canResizeActiveTrack =
    canEditActiveTrack && (!bakeCamera || canPreviewBakedCamera);
  const moveActiveVideoTrack = useCallback(
    (direction: "backward" | "forward") => {
      if (!activeVideoTrack) return;
      const currentIndex = videoTrackOrder.indexOf(activeVideoTrack);
      const nextIndex =
        direction === "forward" ? currentIndex - 1 : currentIndex + 1;
      if (nextIndex < 0 || nextIndex >= videoTrackOrder.length) return;
      const next = [...videoTrackOrder];
      [next[currentIndex], next[nextIndex]] = [
        next[nextIndex],
        next[currentIndex],
      ];
      onVideoTrackOrderChange?.(next);
    },
    [activeVideoTrack, onVideoTrackOrderChange, videoTrackOrder],
  );
  const openLayerContextMenu = useCallback(
    (
      trackId: RecordingVideoTrackId,
      event: ReactMouseEvent<HTMLDivElement>,
    ) => {
      event.preventDefault();
      event.stopPropagation();
      onSelectedTrackChange?.(trackId);
      setLayerContextMenu({
        itemId: trackId,
        x: Math.min(event.clientX, window.innerWidth - 196),
        y: Math.min(event.clientY, window.innerHeight - 92),
      });
    },
    [onSelectedTrackChange],
  );
  const moveVideoTrack = useCallback(
    (trackId: RecordingVideoTrackId, direction: "backward" | "forward") => {
      setLayerContextMenu(null);
      const currentIndex = videoTrackOrder.indexOf(trackId);
      const nextIndex =
        direction === "forward" ? currentIndex - 1 : currentIndex + 1;
      if (nextIndex < 0 || nextIndex >= videoTrackOrder.length) return;
      const next = [...videoTrackOrder];
      [next[currentIndex], next[nextIndex]] = [
        next[nextIndex],
        next[currentIndex],
      ];
      onVideoTrackOrderChange?.(next);
    },
    [onVideoTrackOrderChange, videoTrackOrder],
  );

  // The shortcut hook re-binds its window listener whenever a handler identity
  // changes, so these stay stable across the per-move draft renders.
  const canMoveActiveVideoTrack =
    activeVideoTrack !== null && selectedVideoTracks.has(activeVideoTrack);
  const hasVisiblePanes = visiblePaneEntries.length > 0;
  const moveActiveVideoTrackBackward = useCallback(() => {
    moveActiveVideoTrack("backward");
  }, [moveActiveVideoTrack]);
  const moveActiveVideoTrackForward = useCallback(() => {
    moveActiveVideoTrack("forward");
  }, [moveActiveVideoTrack]);
  const toggleCanvasTool = useCallback(() => {
    setCanvasTool((current) => (current === "canvas" ? null : "canvas"));
  }, []);
  const toggleSelectTool = useCallback(() => {
    setCanvasTool((current) => (current === "select" ? null : "select"));
  }, []);
  const toggleCropTool = useCallback(() => {
    setCanvasTool((current) => (current === "crop" ? null : "crop"));
  }, []);

  useExportWindowShortcuts({
    onMoveBackward: canMoveActiveVideoTrack
      ? moveActiveVideoTrackBackward
      : undefined,
    onMoveForward: canMoveActiveVideoTrack
      ? moveActiveVideoTrackForward
      : undefined,
    onResizeCanvas: canResizeActiveTrack ? toggleCanvasTool : undefined,
    onSelectTool: hasVisiblePanes ? toggleSelectTool : undefined,
    onToggleCrop: hasVisiblePanes ? toggleCropTool : undefined,
    onTogglePlayback: layout ? togglePlayback : undefined,
  });

  // The tools read the committed `recordingOutput`, never the resize draft, so
  // holding the element keeps the memoized toolbar's props stable mid-gesture.
  const cropToggle = useMemo(
    () =>
      visiblePaneEntries.length > 0 ? (
        <RecordingCanvasTools
          activeTrack={activeVideoTrack}
          bakeCamera={bakeCamera}
          cameraPane={cameraPane}
          isEnabled={canEditActiveTrack}
          isFrameEnabled={canResizeActiveTrack}
          isSelectEnabled={visiblePaneEntries.length > 0}
          onCameraOverlayReset={onCameraOverlayChange}
          onChange={onRecordingOutputChange}
          onToolChange={setCanvasTool}
          outputs={recordingOutput}
          screenPane={screenPane}
          tool={canvasTool}
        />
      ) : undefined,
    [
      activeVideoTrack,
      bakeCamera,
      cameraPane,
      canEditActiveTrack,
      canResizeActiveTrack,
      canvasTool,
      onCameraOverlayChange,
      onRecordingOutputChange,
      recordingOutput,
      screenPane,
      visiblePaneEntries.length,
    ],
  );
  const previewBadges = useMemo(
    () =>
      visibleLayout?.panes.map((pane, index) => {
        const outputDimensions =
          previewOutputDimensions?.[visiblePaneEntries[index].trackId];
        return {
          height: outputDimensions?.height ?? pane.sourceHeight,
          kind: pane.kind,
          width: outputDimensions?.width ?? pane.sourceWidth,
        };
      }) ?? [],
    [previewOutputDimensions, visibleLayout, visiblePaneEntries],
  );

  useEffect(() => {
    playhead.publish(0, 0);
  }, [artifactId, playhead]);

  // The native surface owns display, so the DOM canvases carry no pixels.
  // The crop magnifier samples them directly, so while editing is active the
  // current source frame is fetched once and drawn into the (invisible)
  // canvas of the edited pane.
  const editedTrack =
    canvasTool === "crop" && !isScrubbing ? activeVideoTrack : null;
  const getPositionMs = player.getPositionMs;
  useEffect(() => {
    if (!editedTrack) return;
    const isCameraSource = editedTrack === "camera";
    const targetRef = isCameraSource ? cameraCanvasRef : screenCanvasRef;
    let cancelled = false;
    void copyRecordingPreviewSourceFrame({
      artifactId,
      positionMs: getPositionMs(),
      track: isCameraSource ? 1 : 0,
    })
      .then(async (bytes) => {
        const bitmap = await createImageBitmap(
          new Blob([bytes], { type: "image/jpeg" }),
        );
        if (cancelled) return;
        const canvas = targetRef.current;
        if (!canvas) return;
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
        canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      // The canvas only exists as the magnifier's pixel source; a stale
      // frame left behind would be visible in layouts that show the canvas.
      const canvas = targetRef.current;
      if (canvas) {
        canvas.width = 0;
        canvas.height = 0;
      }
    };
  }, [
    artifactId,
    bakeCamera,
    cameraCanvasRef,
    editedTrack,
    getPositionMs,
    screenCanvasRef,
  ]);

  // The lanes are memoized, so the handlers they receive must outlive a resize
  // draft update. The player's seek is re-created per render by design, so it is
  // reached through a ref rather than captured.
  const playerSeekRef = useRef(player.seek);
  playerSeekRef.current = player.seek;
  const seek = useCallback(
    (ratio: number, phase: "end" | "move" | "start") => {
      if (phase === "start") setIsScrubbing(true);
      if (phase === "end") setIsScrubbing(false);
      const positionMs = ratio * totalDurationRef.current;
      playhead.publish(positionMs / 1_000, ratio);
      playerSeekRef.current(positionMs, phase);
    },
    [playhead],
  );
  const changeEnabledTracks = useCallback(
    (tracks: Set<number>) => {
      onEnabledTracksChange?.([...tracks]);
    },
    [onEnabledTracksChange],
  );
  const changeEnabledVideoTracks = useCallback(
    (tracks: Set<RecordingVideoTrackId>) => {
      onEnabledVideoTracksChange?.([...tracks]);
    },
    [onEnabledVideoTracksChange],
  );
  const changeSelectedTrack = useCallback(
    (trackId: RecordingTrackId) => {
      onSelectedTrackChange?.(trackId);
    },
    [onSelectedTrackChange],
  );
  // The copied frame must use the output on screen, which during a resize is
  // the draft; a ref keeps the handler stable without staling the payload.
  const copyPayloadRef = useRef({
    cursorEffects,
    recordingOutput: effectiveRecordingOutput,
  });
  copyPayloadRef.current = {
    cursorEffects,
    recordingOutput: effectiveRecordingOutput,
  };
  const copyCurrentFrame = useCallback(() => {
    setCopyState("copying");
    setCopyError(null);
    void copyRecordingPreviewFrameToClipboard({
      artifactId,
      cursorEffects: copyPayloadRef.current.cursorEffects,
      positionMs: getPositionMs(),
      recordingOutput: copyPayloadRef.current.recordingOutput,
    })
      .then(() => {
        setCopyState("done");
        window.setTimeout(() => {
          setCopyState("idle");
        }, 1_500);
      })
      .catch((cause: unknown) => {
        setCopyState("idle");
        setCopyError(cause instanceof Error ? cause.message : String(cause));
      });
  }, [artifactId, getPositionMs]);

  return (
    <div className="flex min-h-0 grow flex-col">
      <div className="grid min-h-0 grow grid-cols-[clamp(270px,23vw,300px)_minmax(0,1fr)]">
        {inspector}
        <section className="relative flex min-h-0 min-w-0 flex-col">
          <div
            aria-hidden
            className="pointer-events-none absolute inset-0 -z-10 bg-black/5 dark:bg-black/25"
            data-preview-backdrop
          />
          {visibleLayout && visibleLayout.panes.length > 0 ? (
            <PreviewToolbar
              badges={previewBadges}
              center={cropToggle}
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
                  activeTrack={activeVideoTrack}
                  cameraCanvasRef={cameraCanvasRef}
                  cameraPane={cameraPane}
                  controlsVisible={
                    canvasTool !== null &&
                    canvasTool !== "canvas" &&
                    activeVideoTrack === "camera" &&
                    !isPlaying &&
                    !isScrubbing
                  }
                  interactionEnabled={!isPlaying && !isScrubbing}
                  isBusy={
                    previewLayout === undefined &&
                    (player.isPreparing || isPreparingPreview)
                  }
                  onCanvasResizeDraft={(settings) => {
                    // Same contract as the unbaked viewport: the gesture
                    // renders from the draft and commits once on release.
                    setCanvasResizeDraft(
                      settings === null
                        ? null
                        : { ...effectiveRecordingOutput, primary: settings },
                    );
                  }}
                  onInteractionEnd={cameraHistory.endGesture}
                  onInteractionStart={cameraHistory.beginGesture}
                  onNeedFullResolution={player.requestFullResolution}
                  onOutputChange={(settings) =>
                    onRecordingOutputChange?.("primary", settings)
                  }
                  onSelectTrack={(trackId) => {
                    onSelectedTrackChange?.(trackId);
                  }}
                  onSettingsChange={cameraHistory.change}
                  onTrackContextMenu={openLayerContextMenu}
                  onZoomChange={setZoomPercent}
                  outputControlsVisible={
                    canvasTool !== null &&
                    activeVideoTrack === "primary" &&
                    !isPlaying &&
                    !isScrubbing
                  }
                  outputSettings={
                    // The draft-derived output, so the composed frame, camera
                    // overlay geometry and on-screen controls all follow the
                    // resize before it reaches the export window's state.
                    activeRecordingOutput?.primary ?? {
                      backgroundColor: "#171717",
                      backgroundRadiusPercent: 0,
                      backgroundType: "solid",
                      dropShadow: true,
                      height: screenPane.height,
                      meshColors: [],
                      meshLockedColors: [],
                      meshPoints: [],
                      meshSeed: 0,
                      meshWarpPercent: 0,
                      radiusPercent: 0,
                      screenshotCropHeightPercent: 100,
                      screenshotCropWidthPercent: 100,
                      screenshotCropXPercent: 0,
                      screenshotCropYPercent: 0,
                      screenshotImageWidthPercent: 100,
                      screenshotImageXPercent: 50,
                      screenshotImageYPercent: 50,
                      width: screenPane.width,
                    }
                  }
                  screenCanvasRef={screenCanvasRef}
                  screenPane={screenPane}
                  settings={cameraOverlay}
                  tool={canvasTool}
                  zoomPercent={zoomPercent}
                />
              </div>
            ) : visibleLayout &&
              visibleLayout.panes.length > 0 &&
              activeRecordingOutput ? (
              <div className="flex min-h-0 min-w-0 grow flex-col">
                <RecordingOutputPreviewViewport
                  activeTrack={activeVideoTrack}
                  controlsVisible={!isPlaying && !isScrubbing}
                  entries={visiblePaneEntries}
                  onCanvasResizeDraft={(trackId, settings) => {
                    // `effectiveRecordingOutput` already renders from the
                    // draft while the gesture runs, so the other track's
                    // settings carry over from the frame on screen.
                    setCanvasResizeDraft(
                      settings === null
                        ? null
                        : { ...effectiveRecordingOutput, [trackId]: settings },
                    );
                  }}
                  onChange={onRecordingOutputChange}
                  onNeedFullResolution={player.requestFullResolution}
                  onSelectTrack={(trackId) => {
                    onSelectedTrackChange?.(trackId);
                  }}
                  onTrackContextMenu={openLayerContextMenu}
                  onZoomChange={setZoomPercent}
                  outputs={activeRecordingOutput}
                  tool={canvasTool}
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
          {copyError ? (
            <p className="m-0 px-4 pb-2 text-xs text-error">{copyError}</p>
          ) : null}

          {layout ? (
            <RecordingPlaybackControls
              copyState={copyState}
              durationMs={totalDurationMs}
              isPlaying={player.isPlaying}
              onCopyCurrentFrame={copyCurrentFrame}
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
            onEnabledTracksChange={changeEnabledTracks}
            onEnabledVideoTracksChange={changeEnabledVideoTracks}
            onSeek={seek}
            onSelectedTrackChange={changeSelectedTrack}
            onVideoTrackOrderChange={onVideoTrackOrderChange}
            playhead={playhead}
            selectedTrack={selectedTrack}
            thumbnails={timelineThumbnails}
            videoTrackOrder={videoTrackOrderList}
            volumes={audioVolumeByStream}
          />
        )
      ) : null}
      {layerContextMenu ? (
        <LayerContextMenu
          ariaLabel="Video layer actions"
          canDelete={false}
          menu={layerContextMenu}
          onClose={() => {
            setLayerContextMenu(null);
          }}
          onDelete={() => undefined}
          onMoveBackward={() => {
            moveVideoTrack(layerContextMenu.itemId, "backward");
          }}
          onMoveForward={() => {
            moveVideoTrack(layerContextMenu.itemId, "forward");
          }}
          showDelete={false}
        />
      ) : null}
    </div>
  );
}
