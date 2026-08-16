// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Button } from "../../../components/base/button/button";
import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";
import { defaultCameraOverlay } from "../recording-export-settings";
import {
  RecordingOutputSettings,
  ScreenshotOutputSettings,
  ScreenshotWorkspaceOutputSettings,
  resizeScreenshotWorkspaceCentered,
  screenshotWorkspaceItemOutput,
} from "../screenshot-output";
import {
  AudioTrackVolume,
  CameraOverlaySettings,
  CursorEffectSettings,
  ExportArtifact,
  PreparedAudioTrack,
  RecordingPreviewLayout,
  RecordingTrackId,
  RecordingVideoTrackId,
} from "../types";

import { ExportInspector } from "./export-inspector";
import { RecordingSection, ScreenshotSection } from "./export-preview-section";
import { ExportTitlebar } from "./export-titlebar";
import { ScreenshotInspector } from "./screenshot-inspector";

type ExportPanelProps = {
  artifact: ExportArtifact | null;
  directory: string | null;
  fileStem: string;
  audioTrackVolumes?: AudioTrackVolume[];
  bakeCamera?: boolean;
  cameraCompression?: number;
  cameraOverlay?: CameraOverlaySettings;
  cameraResolutionScalePercent?: number;
  collapseAudio?: boolean;
  compression?: number;
  cursorEffects?: CursorEffectSettings;
  enabledAudioTrackCount?: number;
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
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
  onCanvasResize?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onCollapseAudioChange?: (collapse: boolean) => void;
  onCompressionChange?: (compression: number) => void;
  onCopy?: () => void;
  onCursorEffectsChange?: (settings: CursorEffectSettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onEnabledVideoTracksChange?: (tracks: RecordingVideoTrackId[]) => void;
  onFileStemChange?: (fileStem: string) => void;
  onMinimize?: () => void;
  onNeedFullResolution?: () => void;
  onRecordingOutputChange?: (
    trackId: RecordingVideoTrackId,
    settings: ScreenshotOutputSettings,
  ) => void;
  onResolutionScaleChange?: (scale: number) => void;
  onSave?: () => void;
  onScreenshotBackgroundRadiusChange?: (radiusPercent: number) => void;
  onScreenshotBackgroundRadiusChangeEnd?: () => void;
  onScreenshotOutputChange?: (
    settings: ScreenshotOutputSettings,
    itemId?: number,
  ) => void;
  onScreenshotRadiusChange?: (radiusPercent: number) => void;
  onScreenshotRadiusChangeEnd?: () => void;
  onSelectedScreenshotItemChange?: (itemId: number | null) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId) => void;
  onSelectedTrackVolumeChange?: (decibels: number) => void;
  onToggleMaximize?: () => void;
  onVideoTrackOrderChange?: (tracks: RecordingVideoTrackId[]) => void;
  previewUrl?: string | null;
  recordingOutput?: RecordingOutputSettings;
  recordingPreviewError?: string | null;
  recordingPreviewLayout?: RecordingPreviewLayout;
  recordingPreviewTracks?: PreparedAudioTrack[];
  resolutionScalePercent?: number;
  savePhase?: "camera" | "finalizing" | "recording";
  saveProgress?: number | null;
  screenshotOutput?: ScreenshotWorkspaceOutputSettings;
  screenshotRadiusPercent?: number;
  selectedScreenshotItemId?: number | null;
  selectedTrack?: RecordingTrackId | null;
};

export function ExportPanel({
  artifact,
  audioTrackVolumes = [],
  bakeCamera = false,
  cameraCompression = 0,
  cameraOverlay = defaultCameraOverlay(),
  cameraResolutionScalePercent = 100,
  collapseAudio,
  compression = 0,
  cursorEffects = {
    bake: true,
    clickAnimation: true,
    clipAtVideoEdge: false,
    motionBlur: true,
    sizePercent: 100,
    smoothMovement: true,
  },
  directory,
  enabledAudioTrackCount,
  enabledStreamIndices,
  enabledVideoTracks = [],
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
  onCanvasResize,
  onCollapseAudioChange,
  onCompressionChange,
  onCopy,
  onCursorEffectsChange,
  onEnabledTracksChange,
  onEnabledVideoTracksChange,
  onFileStemChange,
  onMinimize,
  onNeedFullResolution,
  onRecordingOutputChange,
  onResolutionScaleChange,
  onSave,
  onScreenshotBackgroundRadiusChange,
  onScreenshotBackgroundRadiusChangeEnd,
  onScreenshotOutputChange,
  onScreenshotRadiusChange,
  onScreenshotRadiusChangeEnd,
  onSelectedScreenshotItemChange,
  onSelectedTrackChange,
  onSelectedTrackVolumeChange,
  onToggleMaximize,
  onVideoTrackOrderChange,
  previewUrl,
  recordingOutput,
  recordingPreviewError,
  recordingPreviewLayout,
  recordingPreviewTracks,
  resolutionScalePercent,
  savePhase = "recording",
  saveProgress = null,
  screenshotOutput,
  selectedScreenshotItemId = null,
  selectedTrack = null,
}: ExportPanelProps) {
  const isRecording = artifact?.kind === "recording";
  const enabledVideoTrackCount = enabledVideoTracks.length;
  const isAudioExport = isRecording && enabledVideoTrackCount === 0;
  const hasExportableContent =
    !isRecording || enabledVideoTrackCount + (enabledAudioTrackCount ?? 0) > 0;
  const exportExtension =
    isRecording && enabledVideoTrackCount === 0 ? "m4a" : undefined;
  const inspector =
    artifact?.kind === "recording" ? (
      <ExportInspector
        artifact={artifact}
        bakeCamera={bakeCamera}
        cameraCompression={cameraCompression}
        cameraOverlay={cameraOverlay}
        cameraResolutionScalePercent={cameraResolutionScalePercent}
        collapseAudio={collapseAudio}
        compression={compression}
        cursorEffects={cursorEffects}
        enabledAudioTrackCount={enabledAudioTrackCount}
        enabledVideoTracks={enabledVideoTracks}
        error={error}
        estimatedSizeBytes={estimatedSizeBytes}
        isEstimatingSize={isEstimatingSize}
        isSaving={isSaving}
        onBakeCameraChange={onBakeCameraChange}
        onCameraCompressionChange={onCameraCompressionChange}
        onCameraOverlayChange={onCameraOverlayChange}
        onCameraResolutionScaleChange={onCameraResolutionScaleChange}
        onCollapseAudioChange={onCollapseAudioChange}
        onCompressionChange={onCompressionChange}
        onCursorEffectsChange={onCursorEffectsChange}
        onRecordingOutputChange={onRecordingOutputChange}
        onResolutionScaleChange={onResolutionScaleChange}
        onSelectedTrackChange={onSelectedTrackChange}
        onSelectedTrackVolumeChange={onSelectedTrackVolumeChange}
        recordingOutput={recordingOutput}
        resolutionScalePercent={resolutionScalePercent}
        selectedTrack={selectedTrack}
        selectedTrackVolume={
          audioTrackVolumes.find(
            (volume) =>
              `audio:${volume.streamIndex.toString()}` === selectedTrack,
          )?.decibels ?? 0
        }
      />
    ) : null;

  return (
    <main className="window-surface relative flex h-screen w-screen flex-col overflow-hidden rounded-[10px] text-content-fg">
      {/* The window background lives on its own layer so the native preview
          panes below the webview can mask holes through it without also
          masking the controls rendered above them. */}
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-content/92"
        data-preview-backdrop
        data-preview-window-backdrop
      />
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
            {isAudioExport
              ? "Saving audio…"
              : isRecording
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
      <ExportTitlebar
        artifact={artifact}
        directory={directory}
        extension={exportExtension}
        fileStem={fileStem}
        hasExportableContent={hasExportableContent}
        isExportPreparationPending={isExportPreparationPending}
        isSaving={isSaving}
        onBrowse={onBrowse}
        onClose={onCancel}
        onCopy={onCopy}
        onExport={onSave}
        onFileStemChange={onFileStemChange}
        onMinimize={onMinimize}
        onToggleMaximize={onToggleMaximize}
      />

      {artifact?.kind === "recording" ? (
        <RecordingSection
          artifact={artifact}
          audioTrackVolumes={audioTrackVolumes}
          bakeCamera={bakeCamera}
          cameraOverlay={cameraOverlay}
          cameraResolutionScalePercent={cameraResolutionScalePercent}
          cursorEffects={cursorEffects}
          enabledStreamIndices={enabledStreamIndices}
          enabledVideoTracks={enabledVideoTracks}
          hasCursorData={artifact.hasCursorData}
          inspector={inspector}
          isPreparingRecordingAudio={isPreparingRecordingAudio}
          isPreparingRecordingPreview={isPreparingRecordingPreview}
          key={artifact.id}
          onCameraOverlayChange={onCameraOverlayChange}
          onEnabledTracksChange={onEnabledTracksChange}
          onEnabledVideoTracksChange={onEnabledVideoTracksChange}
          onRecordingOutputChange={onRecordingOutputChange}
          onSelectedTrackChange={onSelectedTrackChange}
          onVideoTrackOrderChange={onVideoTrackOrderChange}
          recordingOutput={recordingOutput}
          recordingPreviewError={recordingPreviewError}
          recordingPreviewLayout={recordingPreviewLayout}
          recordingPreviewTracks={recordingPreviewTracks}
          resolutionScalePercent={resolutionScalePercent}
          selectedTrack={selectedTrack}
        />
      ) : artifact ? (
        <section className="grid min-h-0 grow grid-cols-[clamp(350px,28vw,400px)_minmax(0,1fr)]">
          {screenshotOutput ? (
            <ScreenshotInspector
              isSaving={isSaving}
              onChange={onScreenshotOutputChange}
              onDimensionsChange={(width, height) => {
                onCanvasResize?.(
                  resizeScreenshotWorkspaceCentered({
                    height,
                    settings: screenshotOutput,
                    sources: artifact.items,
                    width,
                  }),
                );
              }}
              settings={screenshotWorkspaceItemOutput(
                screenshotOutput,
                selectedScreenshotItemId ?? -1,
              )}
              sourceHeight={artifact.height}
              sourceWidth={artifact.width}
            />
          ) : null}
          <ScreenshotSection
            artifact={artifact}
            onBackgroundRadiusChange={onScreenshotBackgroundRadiusChange}
            onBackgroundRadiusChangeEnd={onScreenshotBackgroundRadiusChangeEnd}
            onCanvasResize={onCanvasResize}
            onNeedFullResolution={onNeedFullResolution}
            onOutputChange={onScreenshotOutputChange}
            onRadiusChange={onScreenshotRadiusChange}
            onRadiusChangeEnd={onScreenshotRadiusChangeEnd}
            onSelectedItemChange={onSelectedScreenshotItemChange}
            previewUrl={previewUrl}
            screenshotOutput={screenshotOutput}
            selectedItemId={selectedScreenshotItemId}
          />
        </section>
      ) : (
        <div className="flex min-h-0 grow items-center justify-center text-sm text-muted">
          Nothing to export
        </div>
      )}
    </main>
  );
}
