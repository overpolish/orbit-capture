// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { MouseEvent as ReactMouseEvent, useRef, useState } from "react";

import {
  ScreenshotOutputSettings,
  ScreenshotWorkspaceOutputSettings,
  fitScreenshotWorkspaceToItems,
  screenshotLayout,
  screenshotWorkspaceItemOutput,
  screenshotOutputDimensions,
  uncroppedScreenshotPreviewOutput,
} from "../screenshot-output";
import { usePreviewCapabilities } from "../use-preview-capabilities";
import { useScreenshotPreviewSurface } from "../use-screenshot-preview-surface";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { NativeOutputPreview } from "./native-output-preview";
import { ScreenshotCanvasControl } from "./screenshot-canvas-control";
import { ScreenshotPreviewLayer } from "./screenshot-preview-layer";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

type PreviewViewportProps = {
  alt: string;
  artifactId: number;
  items: { height: number; id: number; width: number }[];
  naturalHeight: number;
  naturalWidth: number;
  isEditing?: boolean;
  isResizingCanvas?: boolean;
  isSelecting?: boolean;
  onBackgroundRadiusChange?: (radiusPercent: number) => void;
  onBackgroundRadiusChangeEnd?: () => void;
  onCanvasResize?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onItemContextMenu?: (
    itemId: number,
    event: ReactMouseEvent<HTMLDivElement>,
  ) => void;
  onItemDeselect?: () => void;
  onItemSelect?: (itemId: number) => void;
  onNeedFullResolution?: () => void;
  onOutputChange?: (
    settings: ScreenshotOutputSettings,
    itemId?: number,
  ) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  onViewportInteraction?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  previewUrl?: string | null;
  screenshotOutput?: ScreenshotWorkspaceOutputSettings;
  selectedItemId?: number | null;
  zoomPercent?: number;
};

export function PreviewViewport({
  alt,
  artifactId,
  isEditing = false,
  isResizingCanvas = false,
  isSelecting = false,
  items,
  naturalHeight,
  naturalWidth,
  onBackgroundRadiusChange,
  onBackgroundRadiusChangeEnd,
  onCanvasResize,
  onItemContextMenu,
  onItemDeselect,
  onItemSelect,
  onNeedFullResolution,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  onViewportInteraction,
  onZoomChange,
  previewUrl,
  screenshotOutput,
  selectedItemId = null,
  zoomPercent,
}: PreviewViewportProps) {
  const outputRef = useRef<HTMLDivElement | null>(null);
  const composedCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const autoFitGestureRef = useRef<{
    initial: ScreenshotWorkspaceOutputSettings;
    used: boolean;
  } | null>(null);
  const [canvasResizeDraft, setCanvasResizeDraft] =
    useState<ScreenshotWorkspaceOutputSettings | null>(null);
  const workspaceOutput =
    (isResizingCanvas ? canvasResizeDraft : null) ?? screenshotOutput;
  const orderedItems = workspaceOutput
    ? workspaceOutput.items
        .map((itemOutput) => items.find((item) => item.id === itemOutput.id))
        .filter((item): item is (typeof items)[number] => item !== undefined)
    : items;
  const output = workspaceOutput
    ? screenshotOutputDimensions(workspaceOutput)
    : { height: naturalHeight, width: naturalWidth };
  // `undefined` until the capability probe resolves: neither preview path is
  // rendered before then, so the viewport never flashes the wrong one.
  const nativePane = usePreviewCapabilities()?.nativeScreenshotPreview;
  const previewOutput =
    workspaceOutput && isEditing && selectedItemId !== null
      ? {
          ...workspaceOutput,
          items: workspaceOutput.items.map((itemOutput) => {
            const item = items.find(
              (candidate) => candidate.id === itemOutput.id,
            );
            return item && item.id === selectedItemId
              ? {
                  ...itemOutput,
                  output: uncroppedScreenshotPreviewOutput(
                    item,
                    itemOutput.output,
                  ),
                }
              : itemOutput;
          }),
        }
      : workspaceOutput;
  useScreenshotPreviewSurface({
    artifactId,
    canvasRef: composedCanvasRef,
    isEnabled: nativePane === true && workspaceOutput !== undefined,
    output: previewOutput,
    paneCount: orderedItems.length,
    // Reordering changes composition, not source ownership. Restart only when
    // the set of uploaded source images changes.
    sourceKey: orderedItems
      .map((item) => item.id)
      .sort((first, second) => first - second)
      .join(":"),
  });
  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => output}
      hideUntilMeasured
      mediaSizeKey={`${output.width.toString()}x${output.height.toString()}`}
      onNeedFullResolution={onNeedFullResolution}
      onViewportInteraction={onViewportInteraction}
      onZoomChange={onZoomChange}
      renderMedia={({
        onMediaResize,
        onMediaResizeEnd,
        onMediaResizeStart,
        onReady,
        ref,
        style,
      }) => (
        <div
          className="absolute shrink-0 select-none"
          onPointerDown={(event) => {
            if (isSelecting && event.button === 0) onItemDeselect?.();
          }}
          ref={(element) => {
            outputRef.current = element;
            ref(element);
          }}
          style={{
            ...style,
            height: `${output.height.toString()}px`,
            width: `${output.width.toString()}px`,
          }}
        >
          <div className="absolute inset-0 overflow-visible bg-transparent">
            {workspaceOutput && nativePane === true ? (
              // A geometry marker: the composed screenshot renders on the
              // native pane surface below the webview, positioned to this
              // element's rect by the layout hook.
              <canvas
                aria-label="Composed output preview"
                className="absolute inset-0 size-full max-w-none opacity-0"
                ref={composedCanvasRef}
                role="img"
              />
            ) : workspaceOutput && nativePane === false ? (
              <NativeOutputPreview
                artifactId={artifactId}
                canvasRef={composedCanvasRef}
                onReady={onReady}
                output={previewOutput ?? workspaceOutput}
              />
            ) : previewUrl && workspaceOutput === undefined ? (
              <img
                alt={alt}
                className="absolute inset-0 size-full"
                draggable={false}
                onLoad={onReady}
                src={previewUrl}
              />
            ) : null}
          </div>
          {workspaceOutput
            ? orderedItems.map((item) => {
                const itemOutput = screenshotWorkspaceItemOutput(
                  workspaceOutput,
                  item.id,
                );
                const selected = item.id === selectedItemId;
                return (
                  <ScreenshotPreviewLayer
                    isCropTarget={isEditing && !selected}
                    isEditing={isEditing && selected}
                    isItemSelected={selected}
                    isSelecting={isSelecting}
                    key={item.id}
                    onItemContextMenu={(event) => {
                      onItemContextMenu?.(item.id, event);
                    }}
                    onItemSelect={() => {
                      onItemSelect?.(item.id);
                    }}
                    onLayoutChange={(change) => {
                      if (change.autoFitStarted) {
                        autoFitGestureRef.current = {
                          initial: workspaceOutput,
                          used: false,
                        };
                      }
                      const gesture = autoFitGestureRef.current;
                      if (!change.autoFitCanvas || !gesture) {
                        if (gesture?.used) onMediaResizeEnd();
                        autoFitGestureRef.current = null;
                        return change.settings;
                      }
                      const fitted = fitScreenshotWorkspaceToItems({
                        initial: gesture.initial,
                        movedItemId: item.id,
                        movedItemOutput: change.settings,
                        sources: orderedItems,
                      });
                      if (!gesture.used) {
                        gesture.used = true;
                        onMediaResizeStart();
                      }
                      onMediaResize(fitted.bounds);
                      onCanvasResize?.(fitted.output);
                      return fitted.movedItemOutput;
                    }}
                    onLayoutInteractionEnd={() => {
                      const gesture = autoFitGestureRef.current;
                      if (gesture?.used) onMediaResizeEnd();
                      autoFitGestureRef.current = null;
                    }}
                    onLayoutInteractionStart={() => {
                      autoFitGestureRef.current = {
                        initial: workspaceOutput,
                        used: false,
                      };
                    }}
                    onOutputChange={(settings) => {
                      onOutputChange?.(settings, item.id);
                    }}
                    onRadiusChange={selected ? onRadiusChange : undefined}
                    onRadiusChangeEnd={selected ? onRadiusChangeEnd : undefined}
                    output={output}
                    outputRef={outputRef}
                    previewUrl={selected ? previewUrl : undefined}
                    radiusPercent={itemOutput.radiusPercent}
                    settings={itemOutput}
                    snapFrames={orderedItems
                      .filter((candidate) => candidate.id !== item.id)
                      .map((candidate) =>
                        screenshotLayout(
                          candidate,
                          output,
                          screenshotWorkspaceItemOutput(
                            workspaceOutput,
                            candidate.id,
                          ),
                        ),
                      )
                      .map((layout) => layout.crop)}
                    source={item}
                  />
                );
              })
            : null}
          {workspaceOutput && isResizingCanvas ? (
            <ScreenshotCanvasControl
              items={items}
              mediaRef={outputRef}
              onBoundsChange={onMediaResize}
              onChange={setCanvasResizeDraft}
              onResizeEnd={(finalOutput) => {
                onMediaResizeEnd();
                onCanvasResize?.(finalOutput);
                setCanvasResizeDraft(null);
              }}
              onResizeStart={onMediaResizeStart}
              output={output}
              settings={workspaceOutput}
            />
          ) : null}
          {workspaceOutput && isEditing ? (
            <ScreenshotRadiusControl
              anchor="top-right"
              height={output.height}
              mediaRef={outputRef}
              onChange={onBackgroundRadiusChange}
              onChangeEnd={onBackgroundRadiusChangeEnd}
              radiusPercent={workspaceOutput.backgroundRadiusPercent}
              width={output.width}
            />
          ) : null}
        </div>
      )}
      resetKey={artifactId}
      zoomPercent={zoomPercent}
    />
  );
}
