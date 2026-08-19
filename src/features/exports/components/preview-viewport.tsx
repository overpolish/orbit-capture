// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useRef, useState } from "react";

import {
  ScreenshotOutputSettings,
  ScreenshotWorkspaceOutputSettings,
  fitScreenshotWorkspaceToItems,
  resizeScreenshotWorkspaceCanvasEdges,
  screenshotLayout,
  screenshotWorkspaceItemOutput,
  screenshotOutputDimensions,
  uncroppedScreenshotPreviewOutput,
} from "../screenshot-output";
import { useExportEditGesture } from "../use-export-edit-history";
import {
  ScreenshotSelectionGestureEvent,
  useScreenshotPreviewSurface,
} from "../use-screenshot-preview-surface";

type PreviewViewportProps = {
  alt: string;
  artifactId: number;
  items: { height: number; id: number; width: number }[];
  naturalHeight: number;
  naturalWidth: number;
  isEditing?: boolean;
  isResizingCanvas?: boolean;
  /**
   * A running save covers the viewport with the progress overlay, whose Cancel
   * button is a DOM control - and the native interaction view is inserted
   * above the webview, so it would swallow that click and pan the workspace
   * instead. Suspending the native editor for the duration of the save hands
   * input back to the webview without giving up the native composition.
   */
  isSaving?: boolean;
  isSelecting?: boolean;
  onBackgroundRadiusChange?: (radiusPercent: number) => void;
  onBackgroundRadiusChangeEnd?: () => void;
  onCanvasResize?: (settings: ScreenshotWorkspaceOutputSettings) => void;
  onItemSelect?: (itemId: number) => void;
  onOutputChange?: (
    settings: ScreenshotOutputSettings,
    itemId?: number,
  ) => void;
  onRadiusChangeEnd?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  screenshotOutput?: ScreenshotWorkspaceOutputSettings;
  selectedItemId?: number | null;
  zoomPercent?: number;
};

const AUTO_FIT_MOVE_EDGE = 1 << 17;
const AUTO_FIT_COMMIT_EDGE = 1 << 18;

export function PreviewViewport({
  alt,
  artifactId,
  isEditing = false,
  isResizingCanvas = false,
  isSaving = false,
  isSelecting = false,
  items,
  naturalHeight,
  naturalWidth,
  onBackgroundRadiusChange,
  onBackgroundRadiusChangeEnd,
  onCanvasResize,
  onItemSelect,
  onOutputChange,
  onRadiusChangeEnd,
  onZoomChange,
  screenshotOutput,
  selectedItemId = null,
  zoomPercent,
}: PreviewViewportProps) {
  const nativeFrameRef = useRef<HTMLDivElement | null>(null);
  const selectionGestureRef = useRef<{
    autoFitCheckpointed: boolean;
    autoFitUsed: boolean;
    itemId: number;
    lastDeltaX: number;
    lastDeltaY: number;
    lastEdges: number;
    lastScale: number;
    operation: ScreenshotSelectionGestureEvent["operation"];
    paneIndex: number;
    snapshot: ScreenshotOutputSettings;
    workspaceSnapshot: ScreenshotWorkspaceOutputSettings;
    lastAutoFitOutput?: ScreenshotWorkspaceOutputSettings;
  } | null>(null);
  const frameGestureRef = useRef<{
    edges: number;
    lastDeltaX: number;
    lastDeltaY: number;
    lastScale: number;
    operation: "frameRadius" | "frameResize";
    snapshot: ScreenshotWorkspaceOutputSettings;
  } | null>(null);
  const editGesture = useExportEditGesture();
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
  const selectedItemIndex =
    selectedItemId === null || !workspaceOutput
      ? -1
      : workspaceOutput.items.findIndex(
          (itemOutput) => itemOutput.id === selectedItemId,
        );
  const selectedItemOutput =
    selectedItemId !== null && workspaceOutput
      ? screenshotWorkspaceItemOutput(workspaceOutput, selectedItemId)
      : undefined;
  const selectedItem =
    selectedItemId === null
      ? undefined
      : items.find((item) => item.id === selectedItemId);
  const frameGesture = (event: ScreenshotSelectionGestureEvent) => {
    if (event.operation !== "frameResize" && event.operation !== "frameRadius")
      return false;
    if (event.phase === "begin") {
      if (!isResizingCanvas || !workspaceOutput) return true;
      frameGestureRef.current = {
        edges: event.edges,
        lastDeltaX: 0,
        lastDeltaY: 0,
        lastScale: event.scale,
        operation: event.operation,
        snapshot: workspaceOutput,
      };
      editGesture.beginGesture();
      return true;
    }
    const active = frameGestureRef.current;
    if (!active || active.operation !== event.operation) return true;
    if (event.phase === "cancel") {
      if (active.operation === "frameRadius") {
        onBackgroundRadiusChange?.(active.snapshot.backgroundRadiusPercent);
        onBackgroundRadiusChangeEnd?.();
      } else {
        onCanvasResize?.(active.snapshot);
        setCanvasResizeDraft(null);
      }
      frameGestureRef.current = null;
      requestAnimationFrame(editGesture.endGesture);
      return true;
    }
    const differsFromLastUpdate =
      Math.abs(event.deltaX - active.lastDeltaX) > 1e-9 ||
      Math.abs(event.deltaY - active.lastDeltaY) > 1e-9 ||
      Math.abs(event.scale - active.lastScale) > 1e-9 ||
      event.edges !== active.edges;
    if (event.phase === "update" || differsFromLastUpdate) {
      if (active.operation === "frameRadius") {
        onBackgroundRadiusChange?.(Math.min(50, Math.max(0, event.scale)));
      } else {
        const next = resizeScreenshotWorkspaceCanvasEdges({
          deltaX: event.deltaX,
          deltaY: event.deltaY,
          edges: event.edges,
          settings: active.snapshot,
          sources: items,
        });
        setCanvasResizeDraft(next);
        onCanvasResize?.(next);
      }
    }
    active.edges = event.edges;
    active.lastDeltaX = event.deltaX;
    active.lastDeltaY = event.deltaY;
    active.lastScale = event.scale;
    if (event.phase === "end") {
      frameGestureRef.current = null;
      requestAnimationFrame(() => {
        setCanvasResizeDraft(null);
        editGesture.endGesture();
      });
      if (active.operation === "frameRadius") onBackgroundRadiusChangeEnd?.();
    }
    return true;
  };
  const selectionGesture = (event: ScreenshotSelectionGestureEvent) => {
    if (frameGesture(event)) return;
    if (event.phase === "begin") {
      const itemOutput = workspaceOutput?.items[event.paneIndex];
      const cropGesture =
        event.operation === "cropMove" || event.operation === "cropResize";
      if (
        (!isSelecting && !(isEditing && cropGesture)) ||
        !workspaceOutput ||
        !itemOutput
      )
        return;
      const snapshot = screenshotWorkspaceItemOutput(
        workspaceOutput,
        itemOutput.id,
      );
      selectionGestureRef.current = {
        autoFitCheckpointed: false,
        autoFitUsed: false,
        itemId: itemOutput.id,
        lastDeltaX: 0,
        lastDeltaY: 0,
        lastEdges: event.edges,
        lastScale: event.scale,
        operation: event.operation,
        paneIndex: event.paneIndex,
        snapshot,
        workspaceSnapshot: workspaceOutput,
      };
      editGesture.beginGesture();
      return;
    }
    const active = selectionGestureRef.current;
    if (
      !active ||
      event.paneIndex !== active.paneIndex ||
      event.operation !== active.operation
    )
      return;
    if (event.phase === "cancel") {
      if (active.autoFitUsed || active.autoFitCheckpointed)
        onCanvasResize?.(active.workspaceSnapshot);
      else onOutputChange?.(active.snapshot, active.itemId);
      selectionGestureRef.current = null;
      requestAnimationFrame(editGesture.endGesture);
      return;
    }
    const autoFitCommit =
      event.operation === "move" && (event.edges & AUTO_FIT_COMMIT_EDGE) !== 0;
    if (autoFitCommit && active.lastAutoFitOutput) {
      const committed = active.lastAutoFitOutput;
      const committedItem = screenshotWorkspaceItemOutput(
        committed,
        active.itemId,
      );
      active.autoFitCheckpointed = true;
      active.autoFitUsed = false;
      active.lastAutoFitOutput = undefined;
      active.lastDeltaX = 0;
      active.lastDeltaY = 0;
      active.lastEdges = event.edges;
      active.lastScale = event.scale;
      active.snapshot = committedItem;
      // The remainder of this pointer gesture is relative to the accepted
      // canvas, but edit history remains open until the one mouse-up.
      active.workspaceSnapshot = committed;
      return;
    }
    const finaliseGestureFrame = () => {
      active.lastDeltaX = event.deltaX;
      active.lastDeltaY = event.deltaY;
      active.lastScale = event.scale;
    };
    const changed =
      Math.abs(event.deltaX) > 1e-9 ||
      Math.abs(event.deltaY) > 1e-9 ||
      ((event.operation === "resize" || event.operation === "cropResize") &&
        (Math.abs(event.scale - 1) > 1e-9 ||
          Math.abs(event.deltaX) > 1e-9 ||
          Math.abs(event.deltaY) > 1e-9)) ||
      (event.operation === "radius" &&
        Math.abs(event.scale - active.lastScale) > 1e-9);
    const differsFromLastUpdate =
      Math.abs(event.deltaX - active.lastDeltaX) > 1e-9 ||
      Math.abs(event.deltaY - active.lastDeltaY) > 1e-9 ||
      event.edges !== active.lastEdges ||
      ((event.operation === "resize" ||
        event.operation === "radius" ||
        event.operation === "cropResize") &&
        Math.abs(event.scale - active.lastScale) > 1e-9);
    // Mouse-up is authoritative even when snapping returns exactly to the
    // gesture snapshot (zero delta / unit scale). A prior live update may
    // still have moved React away from that snapshot, so rejecting the final
    // zero as "unchanged" would push stale geometry back into native layout.
    const shouldApply = event.phase === "end" ? differsFromLastUpdate : changed;
    const cropX = active.snapshot.screenshotCropXPercent + event.deltaX * 100;
    const cropY = active.snapshot.screenshotCropYPercent + event.deltaY * 100;
    let next: ScreenshotOutputSettings;
    if (event.operation === "cropMove") {
      next = {
        ...active.snapshot,
        screenshotCropXPercent: cropX,
        screenshotCropYPercent: cropY,
      };
    } else if (event.operation === "cropResize") {
      let left = active.snapshot.screenshotCropXPercent;
      let top = active.snapshot.screenshotCropYPercent;
      let right = left + active.snapshot.screenshotCropWidthPercent;
      let bottom = top + active.snapshot.screenshotCropHeightPercent;
      if ((event.edges & 1) !== 0) left += event.deltaX * 100;
      if ((event.edges & 2) !== 0) right += event.deltaX * 100;
      if ((event.edges & 4) !== 0) top += event.deltaY * 100;
      if ((event.edges & 8) !== 0) bottom += event.deltaY * 100;
      next = {
        ...active.snapshot,
        screenshotCropHeightPercent: bottom - top,
        screenshotCropWidthPercent: right - left,
        screenshotCropXPercent: left,
        screenshotCropYPercent: top,
      };
    } else if (event.operation === "radius") {
      next = {
        ...active.snapshot,
        radiusPercent: Math.min(50, Math.max(0, event.scale)),
      };
    } else if (event.operation === "resize") {
      const scale = Math.min(8, Math.max(0, event.scale));
      const transform = (
        value: number,
        startFrame: number,
        nextFrame: number,
      ) => {
        if (Math.abs(scale - 1) < 1e-9) return value;
        const anchor = (nextFrame - startFrame * scale) / (1 - scale);
        return anchor + (value - anchor) * scale;
      };
      next = {
        ...active.snapshot,
        screenshotCropHeightPercent:
          active.snapshot.screenshotCropHeightPercent * scale,
        screenshotCropWidthPercent:
          active.snapshot.screenshotCropWidthPercent * scale,
        screenshotCropXPercent: cropX,
        screenshotCropYPercent: cropY,
        screenshotImageWidthPercent:
          active.snapshot.screenshotImageWidthPercent * scale,
        screenshotImageXPercent: transform(
          active.snapshot.screenshotImageXPercent,
          active.snapshot.screenshotCropXPercent,
          cropX,
        ),
        screenshotImageYPercent: transform(
          active.snapshot.screenshotImageYPercent,
          active.snapshot.screenshotCropYPercent,
          cropY,
        ),
      };
    } else {
      next = {
        ...active.snapshot,
        screenshotCropXPercent: cropX,
        screenshotCropYPercent: cropY,
        screenshotImageXPercent:
          active.snapshot.screenshotImageXPercent + event.deltaX * 100,
        screenshotImageYPercent:
          active.snapshot.screenshotImageYPercent + event.deltaY * 100,
      };
    }
    if (shouldApply) {
      const autoFit =
        event.operation === "move" && (event.edges & AUTO_FIT_MOVE_EDGE) !== 0;
      if (autoFit) {
        const fitted = fitScreenshotWorkspaceToItems({
          initial: active.workspaceSnapshot,
          movedItemId: active.itemId,
          movedItemOutput: next,
          sources: orderedItems,
        });
        active.autoFitUsed = true;
        active.lastAutoFitOutput = fitted.output;
        onCanvasResize?.(fitted.output);
      } else if (active.autoFitUsed && event.operation === "move") {
        active.autoFitUsed = false;
        onCanvasResize?.({
          ...active.workspaceSnapshot,
          items: active.workspaceSnapshot.items.map((item) =>
            item.id === active.itemId ? { ...item, output: next } : item,
          ),
        });
      } else {
        onOutputChange?.(next, active.itemId);
      }
    }
    active.lastEdges = event.edges;
    if (event.phase === "update") finaliseGestureFrame();
    if (event.phase === "end") {
      selectionGestureRef.current = null;
      requestAnimationFrame(editGesture.endGesture);
      if (event.operation === "radius") onRadiusChangeEnd?.();
    }
    return;
  };
  const selectionOverlay =
    isResizingCanvas && workspaceOutput
      ? {
          layerId: 0xffffffff,
          paneIndex: 0,
          radiusPercent: workspaceOutput.backgroundRadiusPercent,
          rect: { height: 1, width: 1, x: 0, y: 0 },
        }
      : (isSelecting || isEditing) &&
          selectedItemIndex >= 0 &&
          selectedItem &&
          selectedItemOutput
        ? (() => {
            const layout = screenshotLayout(
              selectedItem,
              output,
              selectedItemOutput,
            );
            return {
              cropMode: isEditing,
              image: {
                height: layout.image.height / Math.max(1, output.height),
                width: layout.image.width / Math.max(1, output.width),
                x: layout.image.x / Math.max(1, output.width),
                y: layout.image.y / Math.max(1, output.height),
              },
              paneIndex: selectedItemIndex,
              radiusPercent: selectedItemOutput.radiusPercent,
              rect: {
                height: layout.crop.height / Math.max(1, output.height),
                width: layout.crop.width / Math.max(1, output.width),
                x: layout.crop.x / Math.max(1, output.width),
                y: layout.crop.y / Math.max(1, output.height),
              },
            };
          })()
        : null;
  const selectionTargets =
    (isSelecting || isEditing) && workspaceOutput
      ? workspaceOutput.items.flatMap((itemOutput, paneIndex) => {
          const item = items.find(
            (candidate) => candidate.id === itemOutput.id,
          );
          if (!item) return [];
          const layout = screenshotLayout(item, output, itemOutput.output);
          return [
            {
              cropMode: isEditing,
              image: {
                height: layout.image.height / Math.max(1, output.height),
                width: layout.image.width / Math.max(1, output.width),
                x: layout.image.x / Math.max(1, output.width),
                y: layout.image.y / Math.max(1, output.height),
              },
              paneIndex,
              radiusPercent: itemOutput.output.radiusPercent,
              rect: {
                height: layout.crop.height / Math.max(1, output.height),
                width: layout.crop.width / Math.max(1, output.width),
                x: layout.crop.x / Math.max(1, output.width),
                y: layout.crop.y / Math.max(1, output.height),
              },
            },
          ];
        })
      : null;
  useScreenshotPreviewSurface({
    artifactId,
    canvasRef: nativeFrameRef,
    interactionOutput: workspaceOutput,
    isEditorSuspended: isSaving,
    isEnabled: workspaceOutput !== undefined,
    onSelectionChange: (paneIndex) => {
      if (paneIndex === null) return;
      const itemOutput = workspaceOutput?.items[paneIndex];
      if (itemOutput) onItemSelect?.(itemOutput.id);
    },
    onSelectionGesture: selectionGesture,
    onZoomChange,
    output: previewOutput,
    paneCount: orderedItems.length,
    selection: selectionOverlay,
    selectionTargets,
    // Reordering changes composition, not source ownership. Restart only when
    // the set of uploaded source images changes.
    sourceKey: orderedItems
      .map((item) => item.id)
      .sort((first, second) => first - second)
      .join(":"),
    zoomPercent,
  });
  return (
    <div
      aria-label={alt}
      className={`relative flex min-h-0 grow overflow-hidden ${isSelecting ? "cursor-move" : "cursor-grab"}`}
      data-recording-preview-viewport
      ref={nativeFrameRef}
      role="img"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-black/5 dark:bg-black/25"
        data-preview-backdrop
      />
    </div>
  );
}
