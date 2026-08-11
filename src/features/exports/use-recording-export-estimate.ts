// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later
/* eslint-disable @eslint-react/set-state-in-effect -- Export estimation owns this external lifecycle. */

import { useEffect, useRef, useState } from "react";

import { estimateRecordingExport } from "./api";
import { mixSignature, VideoExportSettings } from "./recording-export-settings";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  ExportArtifact,
} from "./types";

const ESTIMATE_DEBOUNCE_MS = 450;

export function useRecordingExportEstimate({
  artifact,
  audioTrackVolumes,
  bakeCamera,
  camera,
  cameraOverlay,
  collapseAudio,
  compression,
  enabledStreamIndices,
  includeCamera,
  includePrimaryVideo,
  resolutionScalePercent,
}: {
  artifact: ExportArtifact | null;
  audioTrackVolumes: AudioTrackVolume[];
  bakeCamera: boolean;
  camera: VideoExportSettings;
  cameraOverlay: CameraOverlaySettings;
  collapseAudio: boolean;
  compression: number;
  enabledStreamIndices: number[] | null;
  includeCamera: boolean;
  includePrimaryVideo: boolean;
  resolutionScalePercent: number;
}) {
  const cacheRef = useRef(new Map<string, number>());
  const [activeJobs, setActiveJobs] = useState(0);
  const [state, setState] = useState<{
    bytes: number | null;
    isEstimating: boolean;
    signature: string;
  } | null>(null);
  const enabledSignature = enabledStreamIndices
    ? mixSignature(enabledStreamIndices)
    : null;
  const hasExportableContent =
    includePrimaryVideo ||
    includeCamera ||
    (enabledSignature !== null && enabledSignature !== "silent");
  const signature =
    artifact?.kind === "recording" &&
    enabledSignature !== null &&
    hasExportableContent
      ? [
          artifact.id,
          includePrimaryVideo ? "primary" : "no-primary",
          includeCamera ? "camera" : "no-camera",
          bakeCamera ? "baked" : "separate",
          compression,
          resolutionScalePercent,
          camera.compression,
          camera.resolutionScalePercent,
          enabledSignature,
          cameraOverlay.cameraXPercent,
          cameraOverlay.cameraYPercent,
          cameraOverlay.cameraWidthPercent,
          cameraOverlay.frameHeightPercent,
          cameraOverlay.frameWidthPercent,
          cameraOverlay.frameXPercent,
          cameraOverlay.frameYPercent,
          cameraOverlay.radiusPercent,
          collapseAudio ? "mix" : "separate",
          audioTrackVolumes
            .map(
              (volume) =>
                `${volume.streamIndex.toString()}-${volume.decibels.toString()}`,
            )
            .join(","),
        ].join(":")
      : null;

  useEffect(() => {
    cacheRef.current.clear();
    setState(null);
  }, [artifact]);

  useEffect(() => {
    if (
      artifact?.kind !== "recording" ||
      enabledSignature === null ||
      signature === null
    )
      return;

    const cached = cacheRef.current.get(signature);
    if (cached !== undefined) {
      setState({ bytes: cached, isEstimating: false, signature });
      return;
    }

    setState({ bytes: null, isEstimating: true, signature });
    let disposed = false;
    const delay =
      !bakeCamera && compression === 0 && camera.compression === 0
        ? 0
        : ESTIMATE_DEBOUNCE_MS;
    const timer = window.setTimeout(() => {
      const streamIndices =
        enabledSignature === "silent"
          ? []
          : enabledSignature.split("-").map(Number);
      setActiveJobs((count) => count + 1);
      estimateRecordingExport({
        artifactId: artifact.id,
        audioTrackVolumes,
        bakeCamera,
        cameraCompression: camera.compression,
        cameraOverlay,
        cameraResolutionScalePercent: camera.resolutionScalePercent,
        collapseAudio,
        compression,
        enabledStreamIndices: streamIndices,
        includeCamera,
        includePrimaryVideo,
        resolutionScalePercent,
        screenshotRadiusPercent: 0,
      })
        .then((bytes) => {
          if (disposed) return;
          cacheRef.current.set(signature, bytes);
          setState({ bytes, isEstimating: false, signature });
        })
        .catch((cause: unknown) => {
          if (disposed) return;
          console.error("Could not estimate the recording size", cause);
          setState({ bytes: null, isEstimating: false, signature });
        })
        .finally(() => {
          setActiveJobs((count) => Math.max(0, count - 1));
        });
    }, delay);

    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [
    artifact,
    audioTrackVolumes,
    bakeCamera,
    camera.compression,
    camera.resolutionScalePercent,
    cameraOverlay,
    collapseAudio,
    compression,
    enabledSignature,
    includeCamera,
    includePrimaryVideo,
    resolutionScalePercent,
    signature,
  ]);

  const current =
    signature !== null && state?.signature === signature ? state : null;
  return {
    estimatedSizeBytes: current?.bytes,
    isEstimatingSize:
      artifact?.kind === "recording" &&
      hasExportableContent &&
      (current === null || current.isEstimating),
    isPending: activeJobs > 0,
  };
}
