// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useRef, useState } from "react";

import {
  browseExportDirectory,
  cancelExport,
  cancelExportJob,
  copyExportToClipboard,
  saveExport,
  setExportDirectory,
  setScreenshotRadius,
} from "./api";
import { ExportPanel } from "./components/export-panel";
import {
  cameraExportSettings,
  defaultCameraOverlay,
  recordingSavePlan,
} from "./recording-export-settings";
import { sourceScalePercent } from "./resolution";
import { selectArtifact, selectDirectory, useExportStore } from "./store";
import { useExportPreviewImage } from "./use-export-preview-image";
import { useExportProgress } from "./use-export-progress";
import { useExportWindowSize } from "./use-export-window-size";
import { useRecordingExportEstimate } from "./use-recording-export-estimate";
import { useRecordingExportPreview } from "./use-recording-export-preview";

const DEFAULT_COMPRESSION = 2;
export function ExportWindow() {
  const artifact = useExportStore(selectArtifact);
  const directory = useExportStore(selectDirectory);
  const persistedScreenshotRadius = useExportStore(
    (state) => state.snapshot.screenshotRadiusPercent,
  );
  const [fileStem, setFileStem] = useState("");
  const [collapseAudio, setCollapseAudio] = useState(false);
  const [compression, setCompression] = useState(DEFAULT_COMPRESSION);
  const [cameraCompression, setCameraCompression] =
    useState(DEFAULT_COMPRESSION);
  const [bakeCamera, setBakeCamera] = useState(false);
  const [cameraOverlay, setCameraOverlay] = useState(defaultCameraOverlay);
  const [cameraResolutionScalePercent, setCameraResolutionScalePercent] =
    useState(100);
  const [resolutionScalePercent, setResolutionScalePercent] = useState(100);
  const [screenshotRadiusPercent, setScreenshotRadiusPercent] = useState(0);
  const [isSaving, setIsSaving] = useState(false);
  const [isCancelingSave, setIsCancelingSave] = useState(false);
  const [trackSelection, setTrackSelection] = useState<{
    artifactId: number;
    streamIndices: number[];
  } | null>(null);
  const screenshotRadiusRef = useRef(0);
  const [error, setError] = useState<string | null>(null);

  const suggestedFileStem = artifact?.suggestedFileStem ?? "";
  // Keyed on the capture rather than the object, so a replacement always
  // refetches - including the full-resolution copy, whose cached URL belongs to
  // the previous capture's pixels.
  const artifactId = artifact?.id;
  const saveProgress = useExportProgress(artifactId);
  const { loadFullPreview, previewUrl } = useExportPreviewImage(artifactId);
  const onContentHeightChange = useExportWindowSize(
    artifact?.kind === "recording" && artifact.camera ? 920 : undefined,
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
  const effectiveCollapseAudio =
    collapseAudio && (enabledStreamIndices?.length ?? 0) > 1;
  const { estimatedSizeBytes, isEstimatingSize, isPending } =
    useRecordingExportEstimate({
      artifact,
      bakeCamera,
      camera: cameraExport,
      cameraOverlay,
      collapseAudio: effectiveCollapseAudio,
      compression,
      enabledStreamIndices,
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
    setBakeCamera(false);
    setCameraOverlay(defaultCameraOverlay(artifact));
    setCollapseAudio(false);
    setCompression(canCompress ? DEFAULT_COMPRESSION : 0);
    setCameraCompression(canCompress ? DEFAULT_COMPRESSION : 0);
    setCameraResolutionScalePercent(100);
    setResolutionScalePercent(originalResolutionScale);
    screenshotRadiusRef.current = persistedScreenshotRadius;
    setScreenshotRadiusPercent(persistedScreenshotRadius);
    /* eslint-enable @eslint-react/set-state-in-effect */
  }, [
    artifact,
    artifactId,
    canCompress,
    originalResolutionScale,
    persistedScreenshotRadius,
  ]);

  // A capture taken while the window is open replaces the pending one, so the
  // name follows the new suggestion rather than keeping the old capture's.
  useEffect(() => {
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setFileStem(suggestedFileStem);
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setError(null);
  }, [suggestedFileStem]);

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
      bakeCamera={bakeCamera}
      cameraCompression={cameraCompression}
      cameraOverlay={cameraOverlay}
      cameraResolutionScalePercent={cameraResolutionScalePercent}
      collapseAudio={collapseAudio}
      compression={compression}
      directory={directory}
      enabledAudioTrackCount={enabledStreamIndices?.length ?? 0}
      enabledStreamIndices={enabledStreamIndices ?? undefined}
      error={error}
      estimatedSizeBytes={estimatedSizeBytes}
      fileStem={fileStem}
      isCancelingSave={isCancelingSave}
      isEstimatingSize={isEstimatingSize}
      isExportPreparationPending={
        isPending || isPreparingRecordingPreview || isEstimatingSize
      }
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
      onContentHeightChange={onContentHeightChange}
      onCopy={() => {
        copyExportToClipboard(screenshotRadiusPercent).catch(report("copy"));
      }}
      onEnabledTracksChange={onEnabledTracksChange}
      onFileStemChange={(value) => {
        setFileStem(value);
        setError(null);
      }}
      onNeedFullResolution={loadFullPreview}
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
          bakeCamera,
          camera: cameraExport,
          cameraOverlay,
          collapseAudio,
          compression,
          enabledStreamIndices,
          originalResolutionScale,
          resolutionScalePercent,
        });
        setIsSaving(true);
        setIsCancelingSave(false);
        saveProgress.begin(plan.showsMeasuredProgress);
        setError(null);
        saveExport({
          ...plan.options,
          fileStem,
          screenshotRadiusPercent,
        })
          .then((path) => {
            if (path === null) {
              saveProgress.reset();
              setIsCancelingSave(false);
              setIsSaving(false);
              return;
            }
            saveProgress.complete();
            setIsCancelingSave(false);
            setIsSaving(false);
          })
          .catch(report("save"));
      }}
      onScreenshotRadiusChange={(value) => {
        screenshotRadiusRef.current = value;
        setScreenshotRadiusPercent(value);
        setError(null);
      }}
      onScreenshotRadiusChangeEnd={() => {
        setScreenshotRadius(screenshotRadiusRef.current).catch(
          report("remember the screenshot radius for"),
        );
      }}
      previewUrl={previewUrl}
      recordingPreviewError={recordingPreviewError}
      recordingPreviewTracks={recordingPreviewTracks}
      resolutionScalePercent={resolutionScalePercent}
      savePhase={saveProgress.phase}
      saveProgress={saveProgress.progress}
      screenshotRadiusPercent={screenshotRadiusPercent}
    />
  );
}
