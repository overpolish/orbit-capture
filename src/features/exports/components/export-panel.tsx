// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ArrowRight, ClipboardCopy, Folder, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  Input,
  Label,
  Selection,
  TextField as AriaTextField,
  ToggleButtonGroup,
} from "react-aria-components";

import logoUrl from "../../../assets/orbit-capture-mark.svg";
import { Button } from "../../../components/base/button/button";
import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Checkbox } from "../../../components/base/checkbox/checkbox";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { inputFieldVariants } from "../../../components/base/input-fields/input-field";
import { Overlay } from "../../../components/base/overlay/overlay";
import { formatBytes } from "../duration";
import { resolutionScales, scaledDimensions } from "../resolution";
import { ExportArtifact, PreparedAudioTrack } from "../types";

import { PreviewViewport } from "./preview-viewport";
import { RecordingMetadata, ScrubPreview } from "./scrub-preview";

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

const compressionOptions = [
  { label: "Original", value: 0 },
  { label: "High", value: 1 },
  { label: "Balanced", value: 2 },
  { label: "Smaller", value: 3 },
  { label: "Smallest", value: 4 },
] as const;

const selectedNumber = (selection: Selection) => {
  if (selection === "all") return null;
  const selected = selection.values().next().value;
  return selected === undefined ? null : Number(selected);
};

const formatResolutionRatio = (ratio: number) =>
  ratio.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");

/**
 * The screenshot section. Sibling to `RecordingSection`, and the reason the
 * frame around them does not know what it is showing.
 */
function ScreenshotSection({
  artifact,
  onNeedFullResolution,
  previewUrl,
}: {
  artifact: ExportArtifact;
  onNeedFullResolution?: () => void;
  previewUrl?: string | null;
}) {
  return (
    <div className="flex flex-col gap-2">
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onNeedFullResolution={onNeedFullResolution}
        previewUrl={previewUrl}
      />
      <p className="m-0 text-center text-xxs text-muted tabular-nums">
        {artifact.width} &times; {artifact.height}
      </p>
    </div>
  );
}

/**
 * The recording section: a preview you skim, with what the file is underneath.
 *
 * Framed exactly like the still beside it - no box, no border, just the
 * picture and its shadow - because they are the same kind of thing to the
 * person deciding whether to keep it.
 */
function RecordingSection({
  artifact,
  isPreparingRecordingPreview,
  isRemixingRecordingPreview,
  onEnabledTracksChange,
  previewUrl,
  recordingMixUrl,
  recordingPreviewError,
  recordingPreviewTracks,
  videoUrl,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  isPreparingRecordingPreview?: boolean;
  isRemixingRecordingPreview?: boolean;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  previewUrl?: string | null;
  recordingMixUrl?: string | null;
  recordingPreviewError?: string | null;
  recordingPreviewTracks?: PreparedAudioTrack[];
  videoUrl?: string | null;
}) {
  // A recovered recording is presented knowing none of this, so whatever the
  // file itself reports fills the gap.
  const [discovered, setDiscovered] = useState<RecordingMetadata | null>(null);
  const width = artifact.width || (discovered?.width ?? 0);
  const height = artifact.height || (discovered?.height ?? 0);

  return (
    <div className="flex flex-col gap-2">
      <ScrubPreview
        artifactId={artifact.id}
        audioError={recordingPreviewError}
        audioTracks={recordingPreviewTracks}
        durationMs={artifact.durationMs}
        isPreparingAudio={isPreparingRecordingPreview}
        isRemixing={isRemixingRecordingPreview}
        key={artifact.id}
        mixUrl={recordingMixUrl}
        onEnabledTracksChange={onEnabledTracksChange}
        onMetadata={setDiscovered}
        posterUrl={previewUrl}
        videoUrl={videoUrl}
      />
      <div className="flex items-center justify-center gap-2 text-xxs text-muted tabular-nums">
        {width > 0 && height > 0 ? (
          <span>
            {width} &times; {height}
          </span>
        ) : null}
      </div>
    </div>
  );
}

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
  const styles = inputFieldVariants({ size: "md", variant: "solid" });
  const canSave =
    Boolean(artifact) &&
    fileStem.trim().length > 0 &&
    !isExportPreparationPending &&
    !isSaving;
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
          <div className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between gap-4">
              <span className="shrink-0 text-xs text-content-fg">
                Compression
              </span>
              <ToggleButtonGroup
                aria-label="Compression"
                className="flex shrink-0 gap-1"
                disallowEmptySelection
                isDisabled={!artifact.canCompress || isSaving}
                onSelectionChange={(selection) => {
                  const selected = selectedNumber(selection);
                  if (selected !== null) onCompressionChange?.(selected);
                }}
                selectedKeys={new Set([compression.toString()])}
                selectionMode="single"
              >
                {compressionOptions.map((option) => (
                  <ToggleButton
                    id={option.value.toString()}
                    key={option.value}
                    size="sm"
                  >
                    {option.label}
                  </ToggleButton>
                ))}
              </ToggleButtonGroup>
            </div>
            {availableResolutionScales.length > 1 ? (
              <div className="flex items-center justify-between gap-4">
                <span className="shrink-0 text-xs text-content-fg">
                  Resolution
                </span>
                <ToggleButtonGroup
                  aria-label="Resolution"
                  className="flex shrink-0 gap-1"
                  disallowEmptySelection
                  isDisabled={!artifact.canCompress || isSaving}
                  onSelectionChange={(selection) => {
                    const selected = selectedNumber(selection);
                    if (selected !== null) onResolutionScaleChange?.(selected);
                  }}
                  selectedKeys={new Set([effectiveResolutionScale.toString()])}
                  selectionMode="single"
                >
                  {availableResolutionScales.map((scale, index) => {
                    const dimensions = scaledDimensions(artifact, scale);
                    const visibleLabel =
                      index === 0
                        ? "Original"
                        : `${formatResolutionRatio(scale / availableResolutionScales[0])}×`;
                    const description = `${visibleLabel}, ${dimensions.width.toString()} by ${dimensions.height.toString()} pixels`;

                    return (
                      <ToggleButton
                        aria-label={description}
                        id={scale.toString()}
                        key={scale}
                        size="sm"
                      >
                        {visibleLabel}
                      </ToggleButton>
                    );
                  })}
                </ToggleButtonGroup>
              </div>
            ) : null}
            <div className="mt-1 flex items-center justify-end gap-1.5 text-xxs text-muted tabular-nums">
              <span>{formatBytes(artifact.originalSizeBytes)} original</span>
              <ArrowRight aria-hidden="true" className="shrink-0" size={12} />
              {isEstimatingSize ? (
                <span className="flex items-center gap-1.5">
                  <CircularProgressBar
                    aria-label="Estimating compressed size"
                    isIndeterminate
                    size={12}
                    strokeWidth={10}
                  />
                  Estimating
                </span>
              ) : estimatedSizeBytes ? (
                <span>~{formatBytes(estimatedSizeBytes)} estimated</span>
              ) : (
                <span>Estimate unavailable</span>
              )}
            </div>
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

        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            if (canSave) onSave?.();
          }}
        >
          <AriaTextField
            aria-label="File name"
            className={styles.base()}
            isDisabled={!artifact || isSaving}
            onChange={onFileStemChange}
            value={fileStem}
          >
            <Label className={styles.label()}>Name</Label>
            <div className={styles.field()}>
              <div className={styles.inputWrapper()}>
                <Input className={styles.input()} />
                <span className="shrink-0 text-xs text-muted">
                  .{artifact?.extension ?? "png"}
                </span>
              </div>
            </div>
          </AriaTextField>

          <div className="flex flex-col gap-1">
            <span className={styles.label()}>Where</span>
            <div className="flex items-center gap-2">
              <Folder className="shrink-0 text-muted" size={16} />
              <span
                className="min-w-0 grow truncate text-xs text-muted"
                title={directory ?? undefined}
              >
                {directory ?? "No folder chosen"}
              </span>
              <Button
                isDisabled={isSaving}
                onPress={onBrowse}
                showFocus={false}
                size="sm"
                variant="soft"
              >
                Choose
              </Button>
            </div>
          </div>

          {error ? (
            <p className="m-0 text-xs text-error" role="alert">
              {error}
            </p>
          ) : null}

          <div className="flex shrink-0 items-center gap-2">
            <Button
              className="mr-auto"
              isDisabled={isSaving}
              onPress={onCancel}
              showFocus={false}
              size="sm"
              variant="soft"
            >
              Cancel
            </Button>

            {/* A movie is not something the clipboard can hold. */}
            {isRecording ? null : (
              <Button
                isDisabled={!artifact || isSaving}
                onPress={onCopy}
                showFocus={false}
                size="sm"
                variant="ghost"
              >
                <ClipboardCopy size={16} />
                Copy instead
              </Button>
            )}
            <Button
              color="info"
              isDisabled={!canSave}
              size="sm"
              type="submit"
              variant="solid"
            >
              {isSaving ? "Saving" : "Save"}
            </Button>
          </div>
        </form>
      </div>
    </main>
  );
}
