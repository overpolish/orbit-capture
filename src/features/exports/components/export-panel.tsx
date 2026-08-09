// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { X } from "lucide-react";
import { useEffect, useRef } from "react";

import logoUrl from "../../../assets/orbit-capture-mark.svg";
import { Button } from "../../../components/base/button/button";
import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";
import { resolutionScales } from "../resolution";
import { ExportArtifact, PreparedAudioTrack } from "../types";

import { ExportDestinationForm } from "./export-destination-form";
import { RecordingSection, ScreenshotSection } from "./export-preview-section";
import { RecordingExportOptions } from "./recording-export-options";

type ExportPanelProps = {
  artifact: ExportArtifact | null;
  directory: string | null;
  fileStem: string;
  collapseAudio?: boolean;
  compression?: number;
  enabledAudioTrackCount?: number;
  error?: string | null;
  estimatedSizeBytes?: number | null;
  isCancelingSave?: boolean;
  isEstimatingSize?: boolean;
  isExportPreparationPending?: boolean;
  isPreparingRecordingPreview?: boolean;
  isRemixingRecordingPreview?: boolean;
  isSaving?: boolean;
  onBrowse?: () => void;
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
  previewUrl?: string | null;
  recordingMixUrl?: string | null;
  recordingPreviewError?: string | null;
  recordingPreviewTracks?: PreparedAudioTrack[];
  resolutionScalePercent?: number;
  saveProgress?: number | null;
  videoUrl?: string | null;
};

export function ExportPanel({
  artifact,
  collapseAudio,
  compression = 0,
  directory,
  enabledAudioTrackCount,
  error,
  estimatedSizeBytes,
  fileStem,
  isCancelingSave = false,
  isEstimatingSize,
  isExportPreparationPending,
  isPreparingRecordingPreview,
  isRemixingRecordingPreview,
  isSaving,
  onBrowse,
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
  previewUrl,
  recordingMixUrl,
  recordingPreviewError,
  recordingPreviewTracks,
  resolutionScalePercent,
  saveProgress = null,
  videoUrl,
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
    // `min-h-screen`, never `h-screen`: the window is sized from this content,
    // so the content must be free to be its natural height while it is
    // measured. Constrained to the viewport it reports back whatever the
    // window already is, the measurement can never discover that it wants to
    // be shorter, and the window keeps whatever height it opened with.
    <main className="window-surface relative min-h-screen overflow-hidden rounded-[10px] bg-content/92 p-6 text-content-fg">
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
            Saving {isRecording ? "recording" : "screenshot"}…
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
          <h1 className="pointer-events-none m-0 animate-gradient bg-linear-to-r from-orange-400 to-orange-500 bg-clip-text bg-size-[300%] text-2xl font-bold text-transparent">
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
          <RecordingSection
            artifact={artifact}
            isPreparingRecordingPreview={isPreparingRecordingPreview}
            isRemixingRecordingPreview={isRemixingRecordingPreview}
            key={artifact.id}
            onEnabledTracksChange={onEnabledTracksChange}
            previewUrl={previewUrl}
            recordingMixUrl={recordingMixUrl}
            recordingPreviewError={recordingPreviewError}
            recordingPreviewTracks={recordingPreviewTracks}
            videoUrl={videoUrl}
          />
        ) : artifact ? (
          <ScreenshotSection
            artifact={artifact}
            onNeedFullResolution={onNeedFullResolution}
            previewUrl={previewUrl}
          />
        ) : (
          <div className="flex h-[220px] items-center justify-center rounded-md border border-muted/20 text-sm text-muted">
            Nothing to export
          </div>
        )}

        {artifact?.kind === "recording" ? (
          <RecordingExportOptions
            artifact={artifact}
            availableResolutionScales={availableResolutionScales}
            compression={compression}
            effectiveResolutionScale={effectiveResolutionScale}
            estimatedSizeBytes={estimatedSizeBytes}
            isEstimatingSize={isEstimatingSize}
            isSaving={isSaving}
            onCompressionChange={onCompressionChange}
            onResolutionScaleChange={onResolutionScaleChange}
          />
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
    </main>
  );
}
