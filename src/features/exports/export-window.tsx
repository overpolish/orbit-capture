// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { convertFileSrc, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  currentMonitor,
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  browseExportDirectory,
  cancelExport,
  cancelExportJob,
  copyExportToClipboard,
  estimateRecordingExport,
  getExportPreview,
  getRecordingPreview,
  getRecordingPreviewMix,
  saveExport,
  setExportDirectory,
} from "./api";
import { ExportPanel } from "./components/export-panel";
import { logPreview } from "./diagnostics";
import { sourceScalePercent } from "./resolution";
import { selectArtifact, selectDirectory, useExportStore } from "./store";
import { RecordingPreview } from "./types";

/** Matches the width the window is built with. */
const WINDOW_WIDTH = 560;
/** The root's `p-6` above and below the measured content. */
const WINDOW_PADDING = 48;
/** Keeps the border and shadow clear of the usable screen edges. */
const WINDOW_MARGIN = 24;
/**
 * How long a toggle is left alone before its mix is built.
 *
 * Switching two tracks off is two presses, not two decisions, and each mix is
 * an FFmpeg process over the whole recording. Long enough to let a second
 * press join the first, short enough that a single one feels immediate.
 */
const REMIX_DEBOUNCE_MS = 300;
const ESTIMATE_DEBOUNCE_MS = 450;
const DEFAULT_COMPRESSION = 2;
const EXPORT_PROGRESS_EVENT = "export://progress";
type ExportProgress = {
  artifactId: number;
  processedMs: number;
};
/** The name of a combination of tracks, matching what the backend files use. */
const mixSignature = (streamIndices: number[]) =>
  streamIndices.length > 0
    ? [...streamIndices].sort((a, b) => a - b).join("-")
    : "silent";

export function ExportWindow() {
  const artifact = useExportStore(selectArtifact);
  const directory = useExportStore(selectDirectory);
  const [fileStem, setFileStem] = useState("");
  const [collapseAudio, setCollapseAudio] = useState(false);
  const [compression, setCompression] = useState(DEFAULT_COMPRESSION);
  const [resolutionScalePercent, setResolutionScalePercent] = useState(100);
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [fullPreviewUrl, setFullPreviewUrl] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [isCancelingSave, setIsCancelingSave] = useState(false);
  const [saveProgress, setSaveProgress] = useState<number | null>(null);
  const [recordingPreviewState, setRecordingPreviewState] = useState<{
    artifactId: number;
    error: string | null;
    preview: RecordingPreview | null;
  } | null>(null);
  /** What the window's track rows are set to, once they have been touched. */
  const [trackSelection, setTrackSelection] = useState<{
    artifactId: number;
    streamIndices: number[];
  } | null>(null);
  const [mixUrl, setMixUrl] = useState<string | null>(null);
  const [isRemixing, setIsRemixing] = useState(false);
  /**
   * Set once a mix could not be built at all, which in practice means FFmpeg
   * is not installed. The recording then becomes the preview source again -
   * one audible track is a poor preview, but it is a far better one than a
   * poster nobody can play.
   */
  const [hasMixFailed, setHasMixFailed] = useState(false);
  /**
   * The file already built for each combination of tracks, so switching one
   * off and straight back on is instant rather than another FFmpeg run. The
   * backend keeps the files; this keeps the round trip.
   */
  const mixUrlsRef = useRef(new Map<string, string>());
  const estimateCacheRef = useRef(new Map<string, number>());
  const [estimateState, setEstimateState] = useState<{
    bytes: number | null;
    isEstimating: boolean;
    signature: string;
  } | null>(null);
  const [activeMediaJobs, setActiveMediaJobs] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const suggestedFileStem = artifact?.suggestedFileStem ?? "";
  // Recordings are played from disk rather than shipped over IPC - a movie is
  // far too big to hand across, and the asset protocol is scoped to the one
  // folder they live in.
  const videoUrl =
    artifact?.kind === "recording" && isTauri()
      ? convertFileSrc(artifact.path)
      : null;
  /**
   * Whether the recording is worth playing as it is, or only through a mix.
   *
   * A media element renders the *first* audio track of a file and nothing
   * else, and WebKit does not even list the others: a two-track recording
   * loaded into a WKWebView reports `audioTracks.length` of one, so there is
   * no track to enable from here. Measured against a recording whose
   * system-audio track was written first and captured silence, the element
   * played it at `readyState` 4 from end to end, fired `playing` at once and
   * made no sound at all - the microphone underneath it was never rendered.
   *
   * Falling back to the recording there is what made the preview silent until
   * the mix landed and then abruptly gain sound partway through. So it is
   * offered only when its one track is the whole recording. `plays_without_
   * mixing` in `media_preview.rs` is the same rule on the other side.
   */
  const recordingPlaysUnmixed =
    artifact?.kind === "recording" && artifact.audioTracks.length <= 1;
  // Keyed on the capture rather than the object, so a replacement always
  // refetches - including the full-resolution copy, whose cached URL belongs to
  // the previous capture's pixels.
  const artifactId = artifact?.id;
  const canCompress = artifact?.kind === "recording" && artifact.canCompress;
  const originalResolutionScale =
    artifact?.kind === "recording" ? sourceScalePercent(artifact) : 100;

  useEffect(() => {
    if (artifact?.kind !== "recording" || artifact.audioTracks.length === 0) {
      return;
    }

    let disposed = false;
    void getRecordingPreview(artifact.id)
      .then((preview) => {
        if (!disposed) {
          setRecordingPreviewState({
            artifactId: artifact.id,
            error: null,
            preview,
          });
        }
      })
      .catch((cause: unknown) => {
        if (disposed) return;
        console.error("Could not prepare the recording preview", cause);
        setRecordingPreviewState({
          artifactId: artifact.id,
          error: cause instanceof Error ? cause.message : String(cause),
          preview: null,
        });
      });

    return () => {
      disposed = true;
    };
  }, [artifact]);

  const expectsRecordingPreview =
    artifact?.kind === "recording" && artifact.audioTracks.length > 0;
  const currentRecordingPreviewState =
    expectsRecordingPreview && recordingPreviewState?.artifactId === artifact.id
      ? recordingPreviewState
      : null;
  const recordingPreview = currentRecordingPreviewState?.preview ?? null;
  const recordingPreviewError = currentRecordingPreviewState?.error ?? null;
  const isPreparingRecordingPreview =
    expectsRecordingPreview && currentRecordingPreviewState === null;
  // Held as they arrived rather than rebuilt: this array is a dependency
  // downstream, and a new one every render would retrigger the effect that
  // resets which tracks are enabled, which sets state, which renders again.
  const recordingPreviewTracks = recordingPreview?.tracks;

  // Every recorded track until the user says otherwise. Taken from the
  // artifact rather than from the prepared tracks so the mix can start while
  // the waveforms are still being read, which is usually long enough before
  // anyone presses play that they never see the fallback.
  const enabledStreamIndices =
    artifact?.kind === "recording"
      ? trackSelection?.artifactId === artifact.id
        ? trackSelection.streamIndices
        : artifact.audioTracks.map((track) => track.streamIndex)
      : null;
  // A string, because the array behind it is rebuilt every render and would
  // restart the debounce forever if the effect depended on it.
  const enabledSignature = enabledStreamIndices
    ? mixSignature(enabledStreamIndices)
    : null;
  const effectiveCollapseAudio =
    collapseAudio && (enabledStreamIndices?.length ?? 0) > 1;
  const estimateSignature =
    artifact?.kind === "recording" && enabledSignature !== null
      ? [
          artifact.id,
          compression,
          resolutionScalePercent,
          enabledSignature,
          effectiveCollapseAudio ? "mix" : "separate",
        ].join(":")
      : null;

  const onEnabledTracksChange = useCallback(
    (streamIndices: number[]) => {
      if (artifactId === undefined) return;
      setTrackSelection({ artifactId, streamIndices });
    },
    [artifactId],
  );

  useEffect(() => {
    // The files behind these belong to the artifact that is going away, and
    // the backend has already deleted them.
    mixUrlsRef.current.clear();
    estimateCacheRef.current.clear();
    /* eslint-disable @eslint-react/set-state-in-effect */
    setMixUrl(null);
    setIsRemixing(false);
    setHasMixFailed(false);
    setTrackSelection(null);
    setCollapseAudio(false);
    setCompression(canCompress ? DEFAULT_COMPRESSION : 0);
    setResolutionScalePercent(originalResolutionScale);
    setEstimateState(null);
    /* eslint-enable @eslint-react/set-state-in-effect */
  }, [artifactId, canCompress, originalResolutionScale]);

  useEffect(() => {
    if (
      artifact?.kind !== "recording" ||
      artifact.durationMs <= 0 ||
      artifactId === undefined
    )
      return;

    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ExportProgress>(EXPORT_PROGRESS_EVENT, ({ payload }) => {
      if (disposed || payload.artifactId !== artifactId) return;
      // The atomic rename and final validation happen after FFmpeg reaches the
      // end. Only a successful save is allowed to mean 100%.
      setSaveProgress(
        Math.min(99, (payload.processedMs / artifact.durationMs) * 100),
      );
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [artifact, artifactId]);

  useEffect(() => {
    if (artifactId === undefined || enabledSignature === null) return;

    const cached = mixUrlsRef.current.get(enabledSignature);
    if (cached) {
      setMixUrl(cached);

      setIsRemixing(false);
      return;
    }

    let disposed = false;
    // The first build is the window opening, not a change of mind, so it is
    // started at once. Only a toggle waits to see whether another follows it.
    const delay = mixUrlsRef.current.size === 0 ? 0 : REMIX_DEBOUNCE_MS;
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setIsRemixing(true);
    const timer = setTimeout(() => {
      const streamIndices =
        enabledSignature === "silent"
          ? []
          : enabledSignature.split("-").map(Number);
      logPreview("mix.requested", { artifactId, streamIndices });
      setActiveMediaJobs((count) => count + 1);
      getRecordingPreviewMix(artifactId, streamIndices)
        .then((path) => {
          if (disposed) return;
          const url = isTauri() ? convertFileSrc(path) : path;
          mixUrlsRef.current.set(enabledSignature, url);
          logPreview("mix.ready", { signature: enabledSignature });
          setMixUrl(url);
          setIsRemixing(false);
        })
        .catch((cause: unknown) => {
          if (disposed) return;
          // Deliberately not shown to the user: the preview falls back to the
          // recording, which is still worth watching. Only its first audio
          // track can be heard, which is the whole reason the fallback is not
          // used while a mix is on its way.
          setHasMixFailed(true);
          logPreview("mix.failed", {
            message: cause instanceof Error ? cause.message : String(cause),
            signature: enabledSignature,
          });
          setIsRemixing(false);
        })
        .finally(() => {
          setActiveMediaJobs((count) => Math.max(0, count - 1));
        });
    }, delay);

    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [artifactId, enabledSignature]);

  useEffect(() => {
    if (
      artifact?.kind !== "recording" ||
      enabledSignature === null ||
      estimateSignature === null
    )
      return;

    const cached = estimateCacheRef.current.get(estimateSignature);
    if (cached !== undefined) {
      setEstimateState({
        bytes: cached,
        isEstimating: false,
        signature: estimateSignature,
      });
      return;
    }

    // A source mix also reads the working movie. Waiting avoids competing
    // encodes and, on Windows, ensures Save is never offered while another
    // FFmpeg process still has that file open.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setEstimateState({
      bytes: null,
      isEstimating: true,
      signature: estimateSignature,
    });
    if (isRemixing) return;

    let disposed = false;
    const delay = compression === 0 ? 0 : ESTIMATE_DEBOUNCE_MS;
    const timer = window.setTimeout(() => {
      const streamIndices =
        enabledSignature === "silent"
          ? []
          : enabledSignature.split("-").map(Number);
      setActiveMediaJobs((count) => count + 1);
      estimateRecordingExport({
        artifactId: artifact.id,
        collapseAudio: effectiveCollapseAudio,
        compression,
        enabledStreamIndices: streamIndices,
        resolutionScalePercent,
      })
        .then((bytes) => {
          if (disposed) return;
          estimateCacheRef.current.set(estimateSignature, bytes);
          setEstimateState({
            bytes,
            isEstimating: false,
            signature: estimateSignature,
          });
        })
        .catch((cause: unknown) => {
          if (disposed) return;
          console.error("Could not estimate the recording size", cause);
          setEstimateState({
            bytes: null,
            isEstimating: false,
            signature: estimateSignature,
          });
        })
        .finally(() => {
          setActiveMediaJobs((count) => Math.max(0, count - 1));
        });
    }, delay);

    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [
    artifact,
    compression,
    effectiveCollapseAudio,
    enabledSignature,
    estimateSignature,
    isRemixing,
    resolutionScalePercent,
  ]);

  // A capture taken while the window is open replaces the pending one, so the
  // name follows the new suggestion rather than keeping the old capture's.
  useEffect(() => {
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setFileStem(suggestedFileStem);
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setError(null);
  }, [suggestedFileStem]);

  useEffect(() => {
    if (artifactId === undefined) return;

    let url: string | undefined;
    let disposed = false;

    void getExportPreview()
      .then((bytes) => {
        if (disposed) return;
        url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setPreviewUrl(url);
      })
      .catch((cause: unknown) => {
        console.error("Could not load the export preview", cause);
      });

    return () => {
      disposed = true;
      if (url) URL.revokeObjectURL(url);
      setPreviewUrl(null);
      setFullPreviewUrl(null);
    };
  }, [artifactId]);

  // The full capture is only worth fetching once someone zooms past fit.
  useEffect(() => {
    if (!fullPreviewUrl) return;

    return () => {
      URL.revokeObjectURL(fullPreviewUrl);
    };
  }, [fullPreviewUrl]);

  // The window is sized to whatever the content actually measures, so the
  // spacing between sections stays even instead of one gap absorbing the slack
  // of a hand-picked window height.
  const onContentHeightChange = useCallback((height: number) => {
    if (!isTauri()) return;

    const desiredHeight = Math.ceil(height) + WINDOW_PADDING;
    const target = getCurrentWindow();

    void (async () => {
      // Sizing does not depend on the monitor lookup succeeding. It is only
      // needed to clamp a very tall recording panel onto a short display and
      // to nudge the window back into view afterwards; if it fails, the window
      // must still be the height of its content.
      let monitor = null;
      try {
        monitor = await currentMonitor();
      } catch (cause) {
        console.error("Could not read the current monitor", cause);
      }

      const availableHeight = monitor
        ? monitor.workArea.size.toLogical(monitor.scaleFactor).height -
          WINDOW_MARGIN * 2
        : desiredHeight;
      await target.setSize(
        new LogicalSize(WINDOW_WIDTH, Math.min(desiredHeight, availableHeight)),
      );

      if (!monitor) return;

      const position = await target.outerPosition();
      const size = await target.outerSize();
      const margin = Math.round(WINDOW_MARGIN * monitor.scaleFactor);
      const minimumY = monitor.workArea.position.y + margin;
      const maximumY = Math.max(
        minimumY,
        monitor.workArea.position.y +
          monitor.workArea.size.height -
          margin -
          size.height,
      );
      const y = Math.min(Math.max(position.y, minimumY), maximumY);
      if (y !== position.y) {
        await target.setPosition(new PhysicalPosition(position.x, y));
      }
    })();
  }, []);

  // A recording that needs a mix has nothing playable until one exists, so the
  // window says it is still preparing rather than showing a play button that
  // would produce a picture and no sound.
  const isAwaitingPlayableMix =
    artifact?.kind === "recording" &&
    !recordingPlaysUnmixed &&
    !hasMixFailed &&
    mixUrl === null;
  const playableVideoUrl =
    recordingPlaysUnmixed || hasMixFailed ? videoUrl : null;
  const currentEstimate =
    estimateSignature !== null && estimateState?.signature === estimateSignature
      ? estimateState
      : null;
  const isEstimatingSize =
    artifact?.kind === "recording" &&
    (currentEstimate === null || currentEstimate.isEstimating);

  const report = (action: string) => (cause: unknown) => {
    console.error(`Could not ${action} the export`, cause);
    setError(cause instanceof Error ? cause.message : String(cause));
    setIsSaving(false);
    setIsCancelingSave(false);
    setSaveProgress(null);
  };

  return (
    <ExportPanel
      artifact={artifact}
      collapseAudio={collapseAudio}
      compression={compression}
      directory={directory}
      enabledAudioTrackCount={enabledStreamIndices?.length ?? 0}
      error={error}
      estimatedSizeBytes={currentEstimate?.bytes}
      fileStem={fileStem}
      isCancelingSave={isCancelingSave}
      isEstimatingSize={isEstimatingSize}
      isExportPreparationPending={
        activeMediaJobs > 0 || isRemixing || isEstimatingSize
      }
      isPreparingRecordingPreview={
        isPreparingRecordingPreview || isAwaitingPlayableMix
      }
      // Only while an existing mix is being replaced. The first one is built
      // behind a preview that is already playable, and pulsing at that point
      // would draw the eye to nothing.
      isRemixingRecordingPreview={isRemixing && mixUrl !== null}
      isSaving={isSaving}
      onBrowse={() => {
        browseExportDirectory()
          .then(async (chosen) => {
            if (chosen) await setExportDirectory(chosen);
          })
          .catch(report("choose a folder for"));
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
        copyExportToClipboard().catch(report("copy"));
      }}
      onEnabledTracksChange={onEnabledTracksChange}
      onFileStemChange={(value) => {
        setFileStem(value);
        setError(null);
      }}
      onNeedFullResolution={() => {
        if (fullPreviewUrl) return;

        getExportPreview(true)
          .then((bytes) => {
            setFullPreviewUrl(
              URL.createObjectURL(new Blob([bytes], { type: "image/png" })),
            );
          })
          .catch((cause: unknown) => {
            console.error("Could not load the full-resolution preview", cause);
          });
      }}
      onResolutionScaleChange={(scale) => {
        setResolutionScalePercent(scale);
        if (scale < originalResolutionScale && compression === 0) {
          setCompression(1);
        }
        setError(null);
      }}
      onSave={() => {
        const hasAudioChanges =
          artifact?.kind === "recording" &&
          (enabledStreamIndices?.length !== artifact.audioTracks.length ||
            (collapseAudio && enabledStreamIndices.length > 1));
        const hasMeasuredProgress =
          artifact?.kind === "recording" &&
          artifact.durationMs > 0 &&
          (compression > 0 ||
            resolutionScalePercent < originalResolutionScale ||
            hasAudioChanges);
        setIsSaving(true);
        setIsCancelingSave(false);
        setSaveProgress(hasMeasuredProgress ? 0 : null);
        setError(null);
        saveExport({
          collapseAudio:
            collapseAudio && (enabledStreamIndices?.length ?? 0) > 1,
          compression,
          enabledStreamIndices: enabledStreamIndices ?? [],
          fileStem,
          resolutionScalePercent,
        })
          .then((path) => {
            if (path === null) {
              setSaveProgress(null);
              setIsCancelingSave(false);
              setIsSaving(false);
              return;
            }
            setSaveProgress(100);
            setIsCancelingSave(false);
            setIsSaving(false);
          })
          .catch(report("save"));
      }}
      previewUrl={fullPreviewUrl ?? previewUrl}
      recordingMixUrl={mixUrl}
      recordingPreviewError={recordingPreviewError}
      recordingPreviewTracks={recordingPreviewTracks}
      resolutionScalePercent={resolutionScalePercent}
      saveProgress={saveProgress}
      videoUrl={playableVideoUrl}
    />
  );
}
