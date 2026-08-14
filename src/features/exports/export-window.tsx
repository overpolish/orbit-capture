// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  browseExportDirectory,
  cancelExport,
  cancelExportJob,
  copyExportToClipboard,
  saveExport,
  setExportDirectory,
  setScreenshotBackgroundRadius,
  setScreenshotRadius,
} from "./api";
import { ExportPanel } from "./components/export-panel";
import {
  cameraExportSettings,
  DEFAULT_COMPRESSION,
  DEFAULT_CURSOR_EFFECTS,
  defaultCameraOverlay,
  recordingSavePlan,
} from "./recording-export-settings";
import { sourceScalePercent } from "./resolution";
import {
  defaultRecordingOutput,
  defaultScreenshotOutput,
  RecordingOutputSettings,
  resetScreenshotLayout,
  restoredRecordingOutput,
  ScreenshotOutputSettings,
} from "./screenshot-output";
import { selectArtifact, selectDirectory, useExportStore } from "./store";
import {
  AudioTrackVolume,
  recordingAudioStreamIndex,
  recordingAudioTrackId,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "./types";
import {
  ExportEditState,
  useExportEditHistory,
} from "./use-export-edit-history";
import { useExportPreviewImage } from "./use-export-preview-image";
import { useExportProgress } from "./use-export-progress";
import { useRecordingExportEstimate } from "./use-recording-export-estimate";
import { useRecordingExportPreview } from "./use-recording-export-preview";

const EMPTY_AUDIO_TRACK_VOLUMES: AudioTrackVolume[] = [];

export function ExportWindow() {
  const artifact = useExportStore(selectArtifact);
  const directory = useExportStore(selectDirectory);
  const {
    cursorEffects: persistedCursorEffects,
    recordingOutput: persistedRecordingOutput,
    screenshotBackgroundRadiusPercent: persistedScreenshotBackgroundRadius,
    screenshotOutput: persistedScreenshotOutput,
    screenshotRadiusPercent: persistedScreenshotRadius,
  } = useExportStore((state) => state.snapshot);
  const [fileStem, setFileStem] = useState("");
  const [collapseAudio, setCollapseAudio] = useState(false);
  const [compression, setCompression] = useState(DEFAULT_COMPRESSION);
  const [cameraCompression, setCameraCompression] =
    useState(DEFAULT_COMPRESSION);
  const [bakeCamera, setBakeCamera] = useState(false);
  const [cameraOverlay, setCameraOverlay] = useState(defaultCameraOverlay);
  const [cursorEffects, setCursorEffects] = useState(DEFAULT_CURSOR_EFFECTS);
  const [cameraResolutionScalePercent, setCameraResolutionScalePercent] =
    useState(100);
  const [resolutionScalePercent, setResolutionScalePercent] = useState(100);
  const [screenshotRadiusPercent, setScreenshotRadiusPercent] = useState(0);
  const [screenshotOutput, setScreenshotOutput] =
    useState<ScreenshotOutputSettings>(() => defaultScreenshotOutput(1, 1));
  const [recordingOutput, setRecordingOutput] =
    useState<RecordingOutputSettings>(() =>
      defaultRecordingOutput({ primary: { height: 1, width: 1 } }),
    );
  const [isSaving, setIsSaving] = useState(false);
  const [isCancelingSave, setIsCancelingSave] = useState(false);
  const [trackSelection, setTrackSelection] = useState<{
    artifactId: number;
    streamIndices: number[];
  } | null>(null);
  const [videoTrackSelection, setVideoTrackSelection] = useState<{
    artifactId: number;
    tracks: RecordingVideoTrackId[];
  } | null>(null);
  const [selectedTrack, setSelectedTrack] = useState<{
    artifactId: number;
    trackId: RecordingTrackId;
  } | null>(null);
  const [audioTrackVolumes, setAudioTrackVolumes] = useState<{
    artifactId: number;
    values: AudioTrackVolume[];
  } | null>(null);
  const screenshotRadiusRef = useRef(0);
  const screenshotBackgroundRadiusRef = useRef(0);
  const [error, setError] = useState<string | null>(null);

  // Keyed on the capture rather than the object, so a replacement always
  // refetches - including the full-resolution copy, whose cached URL belongs to
  // the previous capture's pixels.
  const artifactId = artifact?.id;
  const saveProgress = useExportProgress(artifactId);
  const screenshotArtifactId =
    artifact?.kind === "screenshot" ? artifact.id : undefined;
  const { loadFullPreview, previewUrl } = useExportPreviewImage(
    screenshotArtifactId,
    true,
  );
  const canCompress = artifact?.kind === "recording" && artifact.canCompress;
  const originalResolutionScale =
    artifact?.kind === "recording" ? sourceScalePercent(artifact) : 100;
  const cameraExport = cameraExportSettings(
    artifact,
    cameraCompression,
    cameraResolutionScalePercent,
  );
  const shouldPrepareRecordingPreview =
    artifact?.kind === "recording" &&
    (artifact.audioTracks.length > 0 || artifact.camera !== null);
  const {
    error: recordingPreviewError,
    isPreparing: isPreparingRecordingPreview,
    preview: recordingPreview,
  } = useRecordingExportPreview({
    artifactId,
    shouldPrepare: shouldPrepareRecordingPreview,
  });
  // Retain array identity so downstream selection effects do not reset.
  const recordingPreviewTracks = recordingPreview?.tracks;

  // Start with every recorded track until the user changes the selection.
  const enabledStreamIndices =
    artifact?.kind === "recording"
      ? trackSelection?.artifactId === artifact.id
        ? trackSelection.streamIndices
        : artifact.audioTracks.map((track) => track.streamIndex)
      : null;
  const defaultVideoTracks: RecordingVideoTrackId[] =
    artifact?.kind === "recording"
      ? [
          ...(artifact.primaryKind === "audio" ? [] : (["primary"] as const)),
          ...(artifact.camera ? (["camera"] as const) : []),
        ]
      : [];
  const selectedVideoTracks =
    artifact?.kind === "recording" &&
    videoTrackSelection?.artifactId === artifact.id
      ? videoTrackSelection.tracks
      : defaultVideoTracks;
  const enabledVideoTracks =
    defaultVideoTracks.length > 0 && selectedVideoTracks.length === 0
      ? defaultVideoTracks.slice(0, 1)
      : selectedVideoTracks;
  const includePrimaryVideo = enabledVideoTracks.includes("primary");
  const includeCamera = enabledVideoTracks.includes("camera");
  const effectiveBakeCamera =
    bakeCamera && includePrimaryVideo && includeCamera;
  const effectiveCollapseAudio =
    collapseAudio && (enabledStreamIndices?.length ?? 0) > 1;
  const currentAudioTrackVolumes =
    artifact?.kind === "recording" &&
    audioTrackVolumes?.artifactId === artifact.id
      ? audioTrackVolumes.values
      : EMPTY_AUDIO_TRACK_VOLUMES;
  const selectedTrackId: RecordingTrackId | null =
    artifact?.kind === "recording"
      ? selectedTrack?.artifactId === artifact.id &&
        (selectedTrack.trackId === "primary" ||
          selectedTrack.trackId === "camera" ||
          artifact.audioTracks.some(
            (track) =>
              recordingAudioTrackId(track.streamIndex) ===
              selectedTrack.trackId,
          ))
        ? selectedTrack.trackId
        : artifact.primaryKind !== "audio"
          ? "primary"
          : artifact.camera
            ? "camera"
            : artifact.audioTracks[0]
              ? recordingAudioTrackId(artifact.audioTracks[0].streamIndex)
              : null
      : null;
  const selectedStreamIndex = recordingAudioStreamIndex(selectedTrackId);
  const editState = useMemo<ExportEditState>(
    () => ({
      audioTrackVolumes,
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      cursorEffects,
      recordingOutput,
      resolutionScalePercent,
      screenshotOutput,
      trackSelection,
      videoTrackSelection,
    }),
    [
      audioTrackVolumes,
      bakeCamera,
      cameraCompression,
      cameraOverlay,
      cameraResolutionScalePercent,
      collapseAudio,
      compression,
      cursorEffects,
      recordingOutput,
      resolutionScalePercent,
      screenshotOutput,
      trackSelection,
      videoTrackSelection,
    ],
  );
  const applyEditState = useCallback((next: ExportEditState) => {
    setAudioTrackVolumes(next.audioTrackVolumes);
    setBakeCamera(next.bakeCamera);
    setCameraCompression(next.cameraCompression);
    setCameraOverlay(next.cameraOverlay);
    setCameraResolutionScalePercent(next.cameraResolutionScalePercent);
    setCollapseAudio(next.collapseAudio);
    setCompression(next.compression);
    setCursorEffects(next.cursorEffects);
    setResolutionScalePercent(next.resolutionScalePercent);
    setRecordingOutput(next.recordingOutput);
    screenshotRadiusRef.current = next.screenshotOutput.radiusPercent;
    screenshotBackgroundRadiusRef.current =
      next.screenshotOutput.backgroundRadiusPercent;
    setScreenshotRadiusPercent(next.screenshotOutput.radiusPercent);
    setScreenshotOutput(next.screenshotOutput);
    setTrackSelection(next.trackSelection);
    setVideoTrackSelection(next.videoTrackSelection);
    setError(null);
  }, []);
  useExportEditHistory({
    apply: applyEditState,
    resetKey: artifactId,
    state: editState,
  });
  const { estimatedSizeBytes, isEstimatingSize } = useRecordingExportEstimate({
    artifact,
    audioTrackVolumes: currentAudioTrackVolumes,
    bakeCamera: effectiveBakeCamera,
    camera: cameraExport,
    cameraOverlay,
    collapseAudio: effectiveCollapseAudio,
    compression,
    cursorEffects,
    enabledStreamIndices,
    includeCamera,
    includePrimaryVideo,
    recordingOutput,
    resolutionScalePercent,
  });

  const onEnabledTracksChange = useCallback(
    (streamIndices: number[]) => {
      if (artifactId === undefined) return;
      setTrackSelection({ artifactId, streamIndices });
    },
    [artifactId],
  );

  useEffect(() => {
    /* eslint-disable @eslint-react/set-state-in-effect */
    setTrackSelection(null);
    setVideoTrackSelection(null);
    setSelectedTrack(null);
    setAudioTrackVolumes(null);
    setBakeCamera(false);
    setCameraOverlay(defaultCameraOverlay(artifact));
    setCursorEffects(persistedCursorEffects);
    setCollapseAudio(false);
    setCompression(canCompress ? DEFAULT_COMPRESSION : 0);
    setCameraCompression(canCompress ? DEFAULT_COMPRESSION : 0);
    setCameraResolutionScalePercent(100);
    setResolutionScalePercent(originalResolutionScale);
    setRecordingOutput(
      restoredRecordingOutput({
        camera: artifact?.kind === "recording" ? artifact.camera : undefined,
        persisted: persistedRecordingOutput,
        primary: {
          height: artifact?.height ?? 1,
          width: artifact?.width ?? 1,
        },
      }),
    );
    screenshotRadiusRef.current = persistedScreenshotRadius;
    screenshotBackgroundRadiusRef.current = persistedScreenshotBackgroundRadius;
    setScreenshotRadiusPercent(persistedScreenshotRadius);
    const screenshotDefaults = defaultScreenshotOutput(
      artifact?.width ?? 1,
      artifact?.height ?? 1,
      {
        background: persistedScreenshotBackgroundRadius,
        screenshot: persistedScreenshotRadius,
      },
    );
    setScreenshotOutput(
      artifact?.kind === "screenshot" && persistedScreenshotOutput
        ? resetScreenshotLayout(
            {
              ...screenshotDefaults,
              ...persistedScreenshotOutput,
              backgroundRadiusPercent: persistedScreenshotBackgroundRadius,
              // A new capture starts at its own native canvas dimensions.
              // Persist visual preferences, not the previous artifact's
              // aspect ratio or manually enlarged output canvas.
              height: screenshotDefaults.height,
              radiusPercent: persistedScreenshotRadius,
              width: screenshotDefaults.width,
            },
            artifact,
          )
        : artifact?.kind === "screenshot"
          ? resetScreenshotLayout(screenshotDefaults, artifact)
          : screenshotDefaults,
    );
    /* eslint-enable @eslint-react/set-state-in-effect */
    // A cancelled or failed save restores the same artifact through a fresh
    // snapshot. Its controls are still the user's current editing session and
    // must not be reset merely because the object was deserialized again.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [artifactId]);

  // A capture taken while the window is open replaces the pending one, so the
  // name follows the new suggestion rather than keeping the old capture's.
  useEffect(() => {
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setFileStem(artifact?.suggestedFileStem ?? "");
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setError(null);
  }, [artifact?.suggestedFileStem]);

  const report = (action: string) => (cause: unknown) => {
    console.error(`Could not ${action} the export`, cause);
    setError(cause instanceof Error ? cause.message : String(cause));
    setIsSaving(false);
    setIsCancelingSave(false);
    saveProgress.reset();
  };

  return (
    <ExportPanel
      artifact={artifact}
      audioTrackVolumes={currentAudioTrackVolumes}
      bakeCamera={effectiveBakeCamera}
      cameraCompression={cameraCompression}
      cameraOverlay={cameraOverlay}
      cameraResolutionScalePercent={cameraResolutionScalePercent}
      collapseAudio={collapseAudio}
      compression={compression}
      cursorEffects={cursorEffects}
      directory={directory}
      enabledAudioTrackCount={enabledStreamIndices?.length ?? 0}
      enabledStreamIndices={enabledStreamIndices ?? undefined}
      enabledVideoTracks={enabledVideoTracks}
      error={error}
      estimatedSizeBytes={estimatedSizeBytes}
      fileStem={fileStem}
      isCancelingSave={isCancelingSave}
      isEstimatingSize={isEstimatingSize}
      isExportPreparationPending={isPreparingRecordingPreview}
      isPreparingRecordingAudio={isPreparingRecordingPreview}
      isPreparingRecordingPreview={isPreparingRecordingPreview}
      isSaving={isSaving}
      onBakeCameraChange={setBakeCamera}
      onBrowse={() => {
        browseExportDirectory()
          .then(async (chosen) => {
            if (chosen) await setExportDirectory(chosen);
          })
          .catch(report("choose a folder for"));
      }}
      onCameraCompressionChange={(value) => {
        const next = Math.round(value);
        setCameraCompression(next);
        if (next === 0) setCameraResolutionScalePercent(100);
        setError(null);
      }}
      onCameraOverlayChange={setCameraOverlay}
      onCameraResolutionScaleChange={(scale) => {
        setCameraResolutionScalePercent(scale);
        if (scale < 100 && cameraCompression === 0) {
          setCameraCompression(1);
        }
        setError(null);
      }}
      onCancel={() => {
        cancelExport().catch(report("cancel"));
      }}
      onCancelSave={() => {
        setIsCancelingSave(true);
        cancelExportJob()
          .then((accepted) => {
            if (!accepted) setIsCancelingSave(false);
          })
          .catch((cause: unknown) => {
            console.error("Could not cancel the active export", cause);
            setError(cause instanceof Error ? cause.message : String(cause));
            setIsCancelingSave(false);
          });
      }}
      onCollapseAudioChange={setCollapseAudio}
      onCompressionChange={(value) => {
        const next = Math.round(value);
        setCompression(next);
        if (next === 0) setResolutionScalePercent(originalResolutionScale);
        setError(null);
      }}
      onCopy={() => {
        copyExportToClipboard(screenshotOutput).catch(report("copy"));
      }}
      onCursorEffectsChange={setCursorEffects}
      onEnabledTracksChange={onEnabledTracksChange}
      onEnabledVideoTracksChange={(tracks) => {
        if (artifactId === undefined || tracks.length === 0) return;
        setVideoTrackSelection({ artifactId, tracks });
      }}
      onFileStemChange={(value) => {
        setFileStem(value);
        setError(null);
      }}
      onMinimize={() => {
        getCurrentWindow()
          .minimize()
          .catch((cause: unknown) => {
            console.error("Could not minimize the export window", cause);
          });
      }}
      onNeedFullResolution={loadFullPreview}
      onRecordingOutputChange={(trackId, settings) => {
        setRecordingOutput((current) => ({
          ...current,
          [trackId]: { ...settings, backgroundRadiusPercent: 0 },
        }));
        setError(null);
      }}
      onResolutionScaleChange={(scale) => {
        setResolutionScalePercent(scale);
        if (scale < originalResolutionScale && compression === 0) {
          setCompression(1);
        }
        setError(null);
      }}
      onSave={() => {
        const plan = recordingSavePlan({
          artifact,
          audioTrackVolumes: currentAudioTrackVolumes,
          bakeCamera: effectiveBakeCamera,
          camera: cameraExport,
          cameraOverlay,
          collapseAudio,
          compression,
          cursorEffects,
          enabledStreamIndices,
          includeCamera,
          includePrimaryVideo,
          originalResolutionScale,
          recordingOutput,
          resolutionScalePercent,
        });
        setIsSaving(true);
        setIsCancelingSave(false);
        saveProgress.begin(plan.showsMeasuredProgress);
        setError(null);
        saveExport({
          ...plan.options,
          fileStem,
          screenshotOutput,
        })
          .then((path) => {
            if (path === null) {
              saveProgress.reset();
              setIsCancelingSave(false);
              setIsSaving(false);
              return;
            }
            saveProgress.complete();
            // Let the determinate ring visibly reach its completed state.
            // Closing it in the same React batch left the animated stroke at
            // whatever fraction it had reached during the final mux.
            window.setTimeout(() => {
              setIsCancelingSave(false);
              setIsSaving(false);
            }, 200);
          })
          .catch(report("save"));
      }}
      onScreenshotBackgroundRadiusChange={(value) => {
        screenshotBackgroundRadiusRef.current = value;
        setScreenshotOutput((current) => ({
          ...current,
          backgroundRadiusPercent: value,
        }));
        setError(null);
      }}
      onScreenshotBackgroundRadiusChangeEnd={() => {
        setScreenshotBackgroundRadius(
          screenshotBackgroundRadiusRef.current,
        ).catch(report("remember the screenshot background radius for"));
      }}
      onScreenshotOutputChange={(settings) => {
        screenshotRadiusRef.current = settings.radiusPercent;
        setScreenshotRadiusPercent(settings.radiusPercent);
        setScreenshotOutput(settings);
        setError(null);
      }}
      onScreenshotRadiusChange={(value) => {
        screenshotRadiusRef.current = value;
        setScreenshotRadiusPercent(value);
        setScreenshotOutput((current) => ({
          ...current,
          radiusPercent: value,
        }));
        setError(null);
      }}
      onScreenshotRadiusChangeEnd={() => {
        setScreenshotRadius(screenshotRadiusRef.current).catch(
          report("remember the screenshot radius for"),
        );
      }}
      onSelectedTrackChange={(trackId) => {
        if (artifactId === undefined) return;
        setSelectedTrack({ artifactId, trackId });
      }}
      onSelectedTrackVolumeChange={(decibels) => {
        if (artifactId === undefined || selectedStreamIndex === null) return;
        const next = currentAudioTrackVolumes.filter(
          (volume) => volume.streamIndex !== selectedStreamIndex,
        );
        if (decibels !== 0) {
          next.push({
            decibels: Math.round(decibels),
            streamIndex: selectedStreamIndex,
          });
        }
        setAudioTrackVolumes({ artifactId, values: next });
      }}
      onToggleMaximize={() => {
        getCurrentWindow()
          .toggleMaximize()
          .catch((cause: unknown) => {
            console.error(
              "Could not maximize or restore the export window",
              cause,
            );
          });
      }}
      previewUrl={previewUrl}
      recordingOutput={recordingOutput}
      recordingPreviewError={recordingPreviewError}
      recordingPreviewTracks={recordingPreviewTracks}
      resolutionScalePercent={resolutionScalePercent}
      savePhase={saveProgress.phase}
      saveProgress={saveProgress.progress}
      screenshotOutput={screenshotOutput}
      screenshotRadiusPercent={screenshotRadiusPercent}
      selectedTrack={selectedTrackId}
    />
  );
}
