// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { X } from "lucide-react";
import { useEffect, useRef } from "react";

import logoUrl from "../../../assets/orbit-capture-mark.svg";
import { Button } from "../../../components/base/button/button";
import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { OverflowShadow } from "../../../components/base/overflow-shadow/overflow-shadow";
import { Overlay } from "../../../components/base/overlay/overlay";
import { resolutionScales } from "../resolution";
import {
  CameraOverlaySettings,
  ExportArtifact,
  PreparedAudioTrack,
  RecordingPreviewLayout,
} from "../types";

import { ExportDestinationForm } from "./export-destination-form";
import { RecordingSection, ScreenshotSection } from "./export-preview-section";
import { RecordingExportOptions } from "./recording-export-options";

type ExportPanelProps = {
  artifact: ExportArtifact | null;
  directory: string | null;
  fileStem: string;
  bakeCamera?: boolean;
  cameraCompression?: number;
  cameraOverlay?: CameraOverlaySettings;
  cameraResolutionScalePercent?: number;
  collapseAudio?: boolean;
  compression?: number;
  enabledAudioTrackCount?: number;
  enabledStreamIndices?: number[];
  error?: string | null;
  estimatedSizeBytes?: number | null;
  isCancelingSave?: boolean;
  isEstimatingSize?: boolean;
  isExportPreparationPending?: boolean;
  isPreparingRecordingAudio?: boolean;
  isPreparingRecordingPreview?: boolean;
  isSaving?: boolean;
  onBakeCameraChange?: (bake: boolean) => void;
  onBrowse?: () => void;
  onCameraCompressionChange?: (compression: number) => void;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onCameraResolutionScaleChange?: (scale: number) => void;
  onCancel?: () => void;
  onCancelSave?: () => void;
  onCollapseAudioChange?: (collapse: boolean) => void;
  onCompressionChange?: (compression: number) => void;
  onContentHeightChange?: (height: number) => void;
  onCopy?: () => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onFileStemChange?: (fileStem: string) => void;
  onNeedFullResolution?: () => void;
  onResolutionScaleChange?: (scale: number) => void;
  onSave?: () => void;
  onScreenshotRadiusChange?: (radiusPercent: number) => void;
  onScreenshotRadiusChangeEnd?: () => void;
  previewUrl?: string | null;
  recordingPreviewError?: string | null;
  recordingPreviewLayout?: RecordingPreviewLayout;
  recordingPreviewTracks?: PreparedAudioTrack[];
  resolutionScalePercent?: number;
  savePhase?: "camera" | "finalizing" | "recording";
  saveProgress?: number | null;
  screenshotRadiusPercent?: number;
};

export function ExportPanel({
  artifact,
  bakeCamera = false,
  cameraCompression = 0,
  cameraOverlay,
  cameraResolutionScalePercent = 100,
  collapseAudio,
  compression = 0,
  directory,
  enabledAudioTrackCount,
  enabledStreamIndices,
  error,
  estimatedSizeBytes,
  fileStem,
  isCancelingSave = false,
  isEstimatingSize,
  isExportPreparationPending,
  isPreparingRecordingAudio,
  isPreparingRecordingPreview,
  isSaving,
  onBakeCameraChange,
  onBrowse,
  onCameraCompressionChange,
  onCameraOverlayChange,
  onCameraResolutionScaleChange,
  onCancel,
  onCancelSave,
  onCollapseAudioChange,
  onCompressionChange,
  onContentHeightChange,
  onCopy,
  onEnabledTracksChange,
  onFileStemChange,
  onNeedFullResolution,
  onResolutionScaleChange,
  onSave,
  onScreenshotRadiusChange,
  onScreenshotRadiusChangeEnd,
  previewUrl,
  recordingPreviewError,
  recordingPreviewLayout,
  recordingPreviewTracks,
  resolutionScalePercent,
  savePhase = "recording",
  saveProgress = null,
  screenshotRadiusPercent = 0,
}: ExportPanelProps) {
  const isRecording = artifact?.kind === "recording";
  const contentRef = useRef<HTMLDivElement>(null);
  const availableResolutionScales =
    artifact?.kind === "recording" ? resolutionScales(artifact) : [];
  const effectiveResolutionScale =
    artifact?.kind === "recording"
      ? (resolutionScalePercent ?? availableResolutionScales[0])
      : 100;
  useEffect(() => {
    const content = contentRef.current;
    if (!content || !onContentHeightChange) return;

    const observer = new ResizeObserver(() => {
      onContentHeightChange(content.getBoundingClientRect().height);
    });
    observer.observe(content);

    return () => {
      observer.disconnect();
    };
  }, [onContentHeightChange]);

  return (
    // The outer surface follows the native window. The content remains free
    // to take its natural height inside the scroll viewport, which lets the
    // resize observer ask for a smaller window while still making an export
    // with two video previews usable on a shorter display.
    <main className="window-surface relative h-screen overflow-hidden rounded-[10px] bg-content/92 text-content-fg">
      <Overlay blur="lg" contained isOpen={isSaving}>
        <div className="flex flex-col items-center gap-3">
          <CircularProgressBar
            aria-label="Save progress"
            isIndeterminate={saveProgress === null}
            renderLabel={(percentage) =>
              percentage === undefined ? null : (
                <span className="absolute inset-0 flex items-center justify-center text-lg font-semibold text-content-fg tabular-nums">
                  {percentage.toFixed(0)}%
                </span>
              )
            }
            size={96}
            strokeWidth={8}
            value={saveProgress ?? undefined}
          />
          <span className="text-sm text-content-fg">
            {isRecording
              ? savePhase === "camera"
                ? "Saving camera…"
                : savePhase === "finalizing"
                  ? "Finalizing recording…"
                  : "Saving recording…"
              : "Saving screenshot…"}
          </span>
          <Button
            isDisabled={isCancelingSave}
            onPress={onCancelSave}
            showFocus={false}
            size="sm"
            variant="soft"
          >
            {isCancelingSave ? "Canceling…" : "Cancel"}
          </Button>
        </div>
      </Overlay>
      <OverflowShadow className="p-6" rootClassName="h-full" shadowRadius="md">
        <div className="flex flex-col gap-4" ref={contentRef}>
          <header
            className="-m-6 mb-0 flex shrink-0 cursor-grab items-center gap-3 p-6 pb-0"
            data-tauri-drag-region
          >
            <img
              alt="Orbit Capture"
              className="pointer-events-none size-6 shrink-0 brightness-0 dark:invert"
              draggable={false}
              src={logoUrl}
            />
            <h1 className="pointer-events-none m-0 animate-gradient bg-linear-to-r from-sky-400 to-blue-400 bg-clip-text bg-size-[300%] text-2xl font-bold text-transparent">
              {isRecording ? "Save recording" : "Save screenshot"}
            </h1>

            <Button
              aria-label="Close"
              className="group ml-auto cursor-default"
              icon
              onPress={onCancel}
              showFocus={false}
              size="sm"
              variant="ghost"
            >
              <X
                className="origin-center transform-gpu backface-hidden text-muted will-change-transform transition-[color,transform] group-data-[hovered]:scale-110 group-data-[hovered]:text-content-fg"
                size={20}
              />
            </Button>
          </header>

          {artifact?.kind === "recording" ? (
            <RecordingExportOptions
              artifact={artifact}
              availableResolutionScales={availableResolutionScales}
              bakeCamera={bakeCamera}
              cameraCompression={cameraCompression}
              cameraResolutionScale={cameraResolutionScalePercent}
              compression={compression}
              effectiveResolutionScale={effectiveResolutionScale}
              estimatedSizeBytes={estimatedSizeBytes}
              isEstimatingSize={isEstimatingSize}
              isSaving={isSaving}
              onCameraCompressionChange={onCameraCompressionChange}
              onCameraResolutionScaleChange={onCameraResolutionScaleChange}
              onCompressionChange={onCompressionChange}
              onResolutionScaleChange={onResolutionScaleChange}
            />
          ) : null}

          {artifact?.kind === "recording" ? (
            <RecordingSection
              artifact={artifact}
              bakeCamera={bakeCamera}
              cameraOverlay={cameraOverlay}
              enabledStreamIndices={enabledStreamIndices}
              isPreparingRecordingAudio={isPreparingRecordingAudio}
              isPreparingRecordingPreview={isPreparingRecordingPreview}
              key={artifact.id}
              onCameraOverlayChange={onCameraOverlayChange}
              onEnabledTracksChange={onEnabledTracksChange}
              recordingPreviewError={recordingPreviewError}
              recordingPreviewLayout={recordingPreviewLayout}
              recordingPreviewTracks={recordingPreviewTracks}
            />
          ) : artifact ? (
            <ScreenshotSection
              artifact={artifact}
              onNeedFullResolution={onNeedFullResolution}
              onRadiusChange={onScreenshotRadiusChange}
              onRadiusChangeEnd={onScreenshotRadiusChangeEnd}
              previewUrl={previewUrl}
              radiusPercent={screenshotRadiusPercent}
            />
          ) : (
            <div className="flex h-[280px] items-center justify-center rounded-md border border-muted/20 text-sm text-muted">
              Nothing to export
            </div>
          )}

          {isRecording && artifact.camera && cameraOverlay ? (
            <div className="flex flex-col gap-2.5">
              <Checkbox
                isDisabled={isSaving}
                isSelected={bakeCamera}
                onChange={onBakeCameraChange}
                size="sm"
              >
                <span className="flex flex-col">
                  <span className="text-xs">Bake camera into recording</span>
                  <span className="text-xxs text-muted">
                    Drag the camera preview to position it in the recording.
                  </span>
                </span>
              </Checkbox>
            </div>
          ) : null}

          {isRecording && (enabledAudioTrackCount ?? 0) > 1 ? (
            <Checkbox
              isDisabled={isSaving}
              isSelected={collapseAudio}
              onChange={onCollapseAudioChange}
              size="sm"
            >
              <span className="flex flex-col">
                <span className="text-xs">Collapse audio tracks</span>
                <span className="text-xxs text-muted">
                  Mix the selected tracks into one for easier sharing.
                </span>
              </span>
            </Checkbox>
          ) : null}

          <ExportDestinationForm
            artifact={artifact}
            directory={directory}
            error={error}
            fileStem={fileStem}
            isExportPreparationPending={isExportPreparationPending}
            isSaving={isSaving}
            onBrowse={onBrowse}
            onCancel={onCancel}
            onCopy={onCopy}
            onFileStemChange={onFileStemChange}
            onSave={onSave}
          />
        </div>
      </OverflowShadow>
    </main>
  );
}
