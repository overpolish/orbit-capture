// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Crop, MousePointer2, ScanSquare } from "lucide-react";
import { MouseEvent as ReactMouseEvent, ReactNode, useState } from "react";
import { TooltipTrigger } from "react-aria-components";

import { ToggleButton } from "../../../components/base/button/toggle-button";
import { Keyboard } from "../../../components/base/keyboard/keyboard";
import { Tooltip } from "../../../components/base/tooltip/tooltip";
import {
  scaledDimensions,
  scaledVideoDimensions,
  sourceScalePercent,
} from "../resolution";
import {
  RecordingOutputSettings,
  resetScreenshotCrop,
  resetScreenshotTransform,
  resizeScreenshotWorkspaceCentered,
  ScreenshotOutputSettings,
  ScreenshotWorkspaceOutputSettings,
  screenshotOutputDimensions,
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
import { useExportWindowShortcuts } from "../use-export-window-shortcuts";

import { PreviewToolbar } from "./preview-toolbar";
import { PreviewViewport } from "./preview-viewport";
import {
  deleteScreenshotLayer,
  moveScreenshotLayer,
} from "./screenshot-layer-actions";
import {
  ScreenshotLayerContextMenu,
  ScreenshotLayerContextMenuState,
} from "./screenshot-layer-context-menu";
import { ScrubPreview } from "./scrub-preview";

/**
 * The screenshot section. Sibling to `RecordingSection`, and the reason the
 * frame around them does not know what it is showing.
 */
export function ScreenshotSection({
  artifact,
  onBackgroundRadiusChange,
  onBackgroundRadiusChangeEnd,
  onCanvasResize,
  onNeedFullResolution,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  onSelectedItemChange,
  previewUrl,
  screenshotOutput,
  selectedItemId = null,
}: {
  artifact: Extract<ExportArtifact, { kind: "screenshot" }>;
  onBackgroundRadiusChange?: (radiusPercent: number) => void;
  onBackgroundRadiusChangeEnd?: () => void;
  onCanvasResize?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onNeedFullResolution?: () => void;
  onOutputChange?: (
    settings: ScreenshotOutputSettings,
    itemId?: number,
  ) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  onSelectedItemChange?: (itemId: number | null) => void;
  previewUrl?: string | null;
  screenshotOutput?: ScreenshotWorkspaceOutputSettings;
  selectedItemId?: number | null;
}) {
  const [zoomPercent, setZoomPercent] = useState(100);
  const [tool, setTool] = useState<"canvas" | "crop" | "select" | null>(
    "select",
  );
  const [contextMenu, setContextMenu] =
    useState<ScreenshotLayerContextMenuState | null>(null);
  const newestItemId = artifact.items[artifact.items.length - 1]?.id ?? null;
  const moveSelectedLayer = (
    direction: "backward" | "forward",
    itemId = selectedItemId,
  ) => {
    if (!screenshotOutput || itemId === null) return;
    const next = moveScreenshotLayer({
      direction,
      itemId,
      settings: screenshotOutput,
    });
    if (next !== screenshotOutput) onCanvasResize?.(next);
    setContextMenu(null);
  };
  const deleteSelectedLayer = (itemId = selectedItemId) => {
    if (
      !screenshotOutput ||
      itemId === null ||
      screenshotOutput.items.length <= 1
    )
      return;
    const result = deleteScreenshotLayer({
      itemId,
      settings: screenshotOutput,
    });
    if (!result) return;
    onCanvasResize?.(result.settings);
    onSelectedItemChange?.(result.nextSelectedItemId);
    setContextMenu(null);
  };
  useExportWindowShortcuts({
    onDelete: deleteSelectedLayer,
    onMoveBackward: () => {
      moveSelectedLayer("backward");
    },
    onMoveForward: () => {
      moveSelectedLayer("forward");
    },
    onResizeCanvas: () => {
      setTool((current) => (current === "canvas" ? null : "canvas"));
    },
    onSelectTool: () => {
      setTool((current) => (current === "select" ? null : "select"));
    },
    onToggleCrop: () => {
      if (selectedItemId === null) onSelectedItemChange?.(newestItemId);
      setTool((current) => (current === "crop" ? null : "crop"));
    },
  });
  const outputDimensions = screenshotOutput
    ? screenshotOutputDimensions(screenshotOutput)
    : { height: artifact.height, width: artifact.width };
  const selectedItem = artifact.items.find(
    (item) => item.id === selectedItemId,
  );
  const selectedOutput =
    screenshotOutput && selectedItem
      ? screenshotWorkspaceItemOutput(screenshotOutput, selectedItem.id)
      : null;

  return (
    <div className="flex min-h-0 min-w-0 grow flex-col">
      <PreviewToolbar
        badges={[
          {
            height: outputDimensions.height,
            kind: "screenshot",
            width: outputDimensions.width,
          },
        ]}
        center={
          <div className="flex items-center gap-1">
            <TooltipTrigger delay={400}>
              <span
                className="inline-flex"
                onContextMenu={(event: ReactMouseEvent<HTMLSpanElement>) => {
                  event.preventDefault();
                  if (!selectedItem || !selectedOutput) return;
                  onOutputChange?.(
                    resetScreenshotTransform(selectedOutput, selectedItem),
                    selectedItem.id,
                  );
                }}
              >
                <ToggleButton
                  animation="scale-selected"
                  aria-keyshortcuts="V"
                  aria-label="Select screenshot"
                  isSelected={tool === "select"}
                  onChange={(selected) => {
                    setTool(selected ? "select" : null);
                  }}
                  showFocus={false}
                  size="sm"
                  variant="ghost"
                >
                  <MousePointer2 size={15} />
                </ToggleButton>
              </span>
              <Tooltip placement="bottom">
                <span className="flex items-center gap-2">
                  Select
                  <Keyboard size="xs" variant="tooltip">
                    V
                  </Keyboard>
                </span>
              </Tooltip>
            </TooltipTrigger>
            <TooltipTrigger delay={400}>
              <span
                className="inline-flex"
                onContextMenu={(event: ReactMouseEvent<HTMLSpanElement>) => {
                  event.preventDefault();
                  if (!screenshotOutput) return;
                  onCanvasResize?.(
                    resizeScreenshotWorkspaceCentered({
                      height: artifact.height,
                      settings: screenshotOutput,
                      sources: artifact.items,
                      width: artifact.width,
                    }),
                  );
                }}
              >
                <ToggleButton
                  animation="scale-selected"
                  aria-keyshortcuts="F"
                  aria-label="Resize canvas"
                  isSelected={tool === "canvas"}
                  onChange={(selected) => {
                    setTool(selected ? "canvas" : null);
                  }}
                  showFocus={false}
                  size="sm"
                  variant="ghost"
                >
                  <ScanSquare size={15} />
                </ToggleButton>
              </span>
              <Tooltip placement="bottom">
                <span className="flex items-center gap-2">
                  Resize canvas
                  <Keyboard size="xs" variant="tooltip">
                    F
                  </Keyboard>
                </span>
              </Tooltip>
            </TooltipTrigger>
            <TooltipTrigger delay={400}>
              <span
                className="inline-flex"
                onContextMenu={(event: ReactMouseEvent<HTMLSpanElement>) => {
                  event.preventDefault();
                  if (!selectedItem || !selectedOutput) return;
                  onOutputChange?.(
                    resetScreenshotCrop(selectedOutput, selectedItem),
                    selectedItem.id,
                  );
                }}
              >
                <ToggleButton
                  animation="scale-selected"
                  aria-keyshortcuts="C"
                  aria-label="Crop screenshot"
                  isSelected={tool === "crop"}
                  onChange={(selected) => {
                    if (selectedItemId === null)
                      onSelectedItemChange?.(newestItemId);
                    setTool(selected ? "crop" : null);
                  }}
                  showFocus={false}
                  size="sm"
                  variant="ghost"
                >
                  <Crop size={15} />
                </ToggleButton>
              </span>
              <Tooltip placement="bottom">
                <span className="flex items-center gap-2">
                  Crop
                  <Keyboard size="xs" variant="tooltip">
                    C
                  </Keyboard>
                </span>
              </Tooltip>
            </TooltipTrigger>
          </div>
        }
        onZoomChange={(nextZoom) => {
          setContextMenu(null);
          setZoomPercent(nextZoom);
        }}
        zoomPercent={zoomPercent}
      />
      <PreviewViewport
        alt="Screenshot preview"
        artifactId={artifact.id}
        isEditing={tool === "crop"}
        isResizingCanvas={tool === "canvas"}
        isSelecting={tool === "select"}
        items={artifact.items}
        naturalHeight={artifact.height}
        naturalWidth={artifact.width}
        onBackgroundRadiusChange={onBackgroundRadiusChange}
        onBackgroundRadiusChangeEnd={onBackgroundRadiusChangeEnd}
        onCanvasResize={onCanvasResize}
        onItemContextMenu={(itemId, event) => {
          event.preventDefault();
          event.stopPropagation();
          onSelectedItemChange?.(itemId);
          setContextMenu({
            itemId,
            x: Math.min(event.clientX, window.innerWidth - 196),
            y: Math.min(event.clientY, window.innerHeight - 132),
          });
        }}
        onItemSelect={onSelectedItemChange}
        onNeedFullResolution={onNeedFullResolution}
        onOutputChange={onOutputChange}
        onRadiusChange={onRadiusChange}
        onRadiusChangeEnd={onRadiusChangeEnd}
        onViewportInteraction={() => {
          setContextMenu(null);
        }}
        onZoomChange={(nextZoom) => {
          setContextMenu(null);
          setZoomPercent(nextZoom);
        }}
        previewUrl={previewUrl}
        screenshotOutput={screenshotOutput}
        selectedItemId={selectedItemId}
        zoomPercent={zoomPercent}
      />
      {contextMenu ? (
        <ScreenshotLayerContextMenu
          canDelete={(screenshotOutput?.items.length ?? 0) > 1}
          menu={contextMenu}
          onClose={() => {
            setContextMenu(null);
          }}
          onDelete={() => {
            deleteSelectedLayer(contextMenu.itemId);
          }}
          onMoveBackward={() => {
            moveSelectedLayer("backward", contextMenu.itemId);
          }}
          onMoveForward={() => {
            moveSelectedLayer("forward", contextMenu.itemId);
          }}
        />
      ) : null}
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
export function RecordingSection({
  artifact,
  audioTrackVolumes,
  bakeCamera,
  cameraOverlay,
  cameraResolutionScalePercent,
  cursorEffects,
  enabledStreamIndices,
  enabledVideoTracks,
  hasCursorData,
  inspector,
  isPreparingRecordingAudio,
  isPreparingRecordingPreview,
  onCameraOverlayChange,
  onEnabledTracksChange,
  onEnabledVideoTracksChange,
  onRecordingOutputChange,
  onSelectedTrackChange,
  onVideoTrackOrderChange,
  recordingOutput,
  recordingPreviewError,
  recordingPreviewLayout,
  recordingPreviewTracks,
  resolutionScalePercent,
  selectedTrack,
}: {
  artifact: Extract<ExportArtifact, { kind: "recording" }>;
  audioTrackVolumes?: AudioTrackVolume[];
  bakeCamera?: boolean;
  cameraOverlay?: CameraOverlaySettings;
  cameraResolutionScalePercent?: number;
  cursorEffects?: CursorEffectSettings;
  enabledStreamIndices?: number[];
  enabledVideoTracks?: RecordingVideoTrackId[];
  hasCursorData?: boolean;
  inspector?: ReactNode;
  isPreparingRecordingAudio?: boolean;
  isPreparingRecordingPreview?: boolean;
  onCameraOverlayChange?: (settings: CameraOverlaySettings) => void;
  onEnabledTracksChange?: (streamIndices: number[]) => void;
  onEnabledVideoTracksChange?: (tracks: RecordingVideoTrackId[]) => void;
  onRecordingOutputChange?: (
    trackId: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId],
  ) => void;
  onSelectedTrackChange?: (trackId: RecordingTrackId | null) => void;
  onVideoTrackOrderChange?: (tracks: RecordingVideoTrackId[]) => void;
  recordingOutput?: RecordingOutputSettings;
  recordingPreviewError?: string | null;
  recordingPreviewLayout?: RecordingPreviewLayout;
  recordingPreviewTracks?: PreparedAudioTrack[];
  resolutionScalePercent?: number;
  selectedTrack?: RecordingTrackId | null;
}) {
  const primaryOutputDimensions = recordingOutput
    ? {
        height: recordingOutput.primary.height,
        width: recordingOutput.primary.width,
      }
    : scaledDimensions(
        artifact,
        resolutionScalePercent ?? sourceScalePercent(artifact),
      );
  const cameraOutputDimensions = recordingOutput
    ? {
        height: recordingOutput.camera.height,
        width: recordingOutput.camera.width,
      }
    : artifact.camera
      ? scaledVideoDimensions({
          height: artifact.camera.height,
          scale: cameraResolutionScalePercent ?? 100,
          sourceScale: 100,
          width: artifact.camera.width,
        })
      : undefined;

  return (
    <div className="flex min-h-0 grow flex-col">
      <ScrubPreview
        artifactId={artifact.id}
        audioError={recordingPreviewError}
        audioTracks={recordingPreviewTracks}
        audioTrackVolumes={audioTrackVolumes}
        bakeCamera={bakeCamera}
        cameraOverlay={cameraOverlay}
        cursorEffects={cursorEffects}
        durationMs={artifact.durationMs}
        enabledStreamIndices={enabledStreamIndices}
        enabledVideoTracks={enabledVideoTracks}
        hasCursorData={hasCursorData}
        inspector={inspector}
        isPreparingAudio={isPreparingRecordingAudio}
        isPreparingPreview={isPreparingRecordingPreview}
        key={artifact.id}
        onCameraOverlayChange={onCameraOverlayChange}
        onEnabledTracksChange={onEnabledTracksChange}
        onEnabledVideoTracksChange={onEnabledVideoTracksChange}
        onRecordingOutputChange={onRecordingOutputChange}
        onSelectedTrackChange={onSelectedTrackChange}
        onVideoTrackOrderChange={onVideoTrackOrderChange}
        previewLayout={recordingPreviewLayout}
        previewOutputDimensions={{
          primary: primaryOutputDimensions,
          ...(cameraOutputDimensions ? { camera: cameraOutputDimensions } : {}),
        }}
        previewSourceDimensions={{
          primary: { height: artifact.height, width: artifact.width },
          ...(artifact.camera
            ? {
                camera: {
                  height: artifact.camera.height,
                  width: artifact.camera.width,
                },
              }
            : {}),
        }}
        recordingOutput={recordingOutput}
        selectedTrack={selectedTrack}
      />
    </div>
  );
}
