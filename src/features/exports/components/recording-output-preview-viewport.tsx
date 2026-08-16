// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { MouseEvent as ReactMouseEvent, RefObject, useRef } from "react";

import {
  RecordingOutputSettings,
  screenshotOutputDimensions,
  screenshotWorkspaceItemOutput,
} from "../screenshot-output";
import { RecordingPreviewPane, RecordingVideoTrackId } from "../types";
import { usePreviewCapabilities } from "../use-preview-capabilities";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { RecordingCanvasTool } from "./recording-crop-toggle";
import { ScreenshotCanvasControl } from "./screenshot-canvas-control";
import { ScreenshotPreviewLayer } from "./screenshot-preview-layer";

const PANE_GAP = 24;

type Entry = {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  pane: RecordingPreviewPane;
  trackId: RecordingVideoTrackId;
};

function RecordingOutputPane({
  controlsVisible,
  entry,
  isActive,
  onBoundsChange,
  onCanvasResizeDraft,
  onChange,
  onMediaResizeEnd,
  onMediaResizeStart,
  onSelect,
  onTrackContextMenu,
  settings,
  tool,
}: {
  controlsVisible: boolean;
  entry: Entry;
  isActive: boolean;
  settings: RecordingOutputSettings[RecordingVideoTrackId];
  tool: RecordingCanvasTool;
  onBoundsChange?: (bounds: {
    height: number;
    originX: number;
    originY: number;
    width: number;
  }) => void;
  onCanvasResizeDraft?: (
    settings: RecordingOutputSettings[RecordingVideoTrackId] | null,
  ) => void;
  onChange?: (settings: RecordingOutputSettings[RecordingVideoTrackId]) => void;
  onMediaResizeEnd?: () => void;
  onMediaResizeStart?: () => void;
  onSelect?: () => void;
  onTrackContextMenu?: (event: ReactMouseEvent<HTMLDivElement>) => void;
}) {
  const outputRef = useRef<HTMLDivElement | null>(null);
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  const output = screenshotOutputDimensions(settings);
  const source = {
    height: entry.pane.sourceHeight,
    width: entry.pane.sourceWidth,
  };
  const workspace = workspaceFor(settings);
  return (
    <div
      className="absolute select-none"
      onContextMenu={onTrackContextMenu}
      ref={outputRef}
      style={{ height: output.height, width: output.width }}
    >
      <canvas
        aria-label={`${entry.pane.kind === "camera" ? "Camera" : "Screen"} composed preview`}
        className={`absolute inset-0 size-full max-w-none ${nativeSurface ? "opacity-0" : ""}`}
        ref={entry.canvasRef}
        role="img"
      />
      <ScreenshotPreviewLayer
        isCropTarget={controlsVisible && !isActive && tool === "crop"}
        isEditing={controlsVisible && isActive && tool === "crop"}
        isItemSelected={isActive}
        isSelecting={controlsVisible && tool === "select"}
        onItemSelect={onSelect}
        onOutputChange={onChange}
        onRadiusChange={(radiusPercent) => {
          onChange?.({ ...settings, radiusPercent });
        }}
        output={output}
        outputRef={outputRef}
        previewCanvasRef={entry.canvasRef}
        radiusPercent={settings.radiusPercent}
        settings={settings}
        source={{
          height: source.height,
          width: source.width,
        }}
      />
      {controlsVisible && tool === "canvas" ? (
        <ScreenshotCanvasControl
          items={[{ ...source, id: 0 }]}
          mediaRef={outputRef}
          onBoundsChange={onBoundsChange}
          // Every pointer move commits only to the local resize draft: the
          // export window's global state re-renders the whole editor, which
          // starves the native pane's layout loop mid-drag.
          onChange={(next) => {
            onCanvasResizeDraft?.(screenshotWorkspaceItemOutput(next, 0));
          }}
          onResizeEnd={(next) => {
            onMediaResizeEnd?.();
            onChange?.(screenshotWorkspaceItemOutput(next, 0));
            onCanvasResizeDraft?.(null);
          }}
          onResizeStart={() => {
            onSelect?.();
            onMediaResizeStart?.();
          }}
          output={output}
          settings={workspace}
        />
      ) : null}
    </div>
  );
}

const workspaceFor = (
  settings: RecordingOutputSettings[RecordingVideoTrackId],
) => ({ ...settings, items: [{ id: 0, output: settings }] });

export function RecordingOutputPreviewViewport({
  activeTrack,
  controlsVisible,
  entries,
  onCanvasResizeDraft,
  onChange,
  onNeedFullResolution,
  onSelectTrack,
  onTrackContextMenu,
  onZoomChange,
  outputs,
  tool,
  zoomPercent,
}: {
  activeTrack: RecordingVideoTrackId | null;
  controlsVisible: boolean;
  entries: Entry[];
  outputs: RecordingOutputSettings;
  tool: RecordingCanvasTool;
  onCanvasResizeDraft?: (
    trackId: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId] | null,
  ) => void;
  onChange?: (
    trackId: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId],
  ) => void;
  onNeedFullResolution?: () => void;
  onSelectTrack?: (trackId: RecordingVideoTrackId) => void;
  onTrackContextMenu?: (
    trackId: RecordingVideoTrackId,
    event: ReactMouseEvent<HTMLDivElement>,
  ) => void;
  onZoomChange?: (zoomPercent: number) => void;
  zoomPercent?: number;
}) {
  // Canvas-resize bounds arrive in pane coordinates, cumulative from the
  // gesture start, but the viewport compensates in media-box coordinates -
  // and this component re-renders with the committed settings on every move,
  // so the conversion context must be frozen when the gesture begins.
  const resizeBaselineRef = useRef<{
    boxHeight: number;
    boxWidth: number;
    otherMaxHeight: number;
    paneHeight: number;
    paneWidth: number;
  } | null>(null);
  const dimensions = entries.map(({ trackId }) =>
    screenshotOutputDimensions(outputs[trackId]),
  );
  const height = dimensions.reduce(
    (maximum, size) => Math.max(maximum, size.height),
    0,
  );
  const width = Math.max(
    1,
    dimensions.reduce((total, size) => total + size.width, 0) +
      Math.max(0, entries.length - 1) * PANE_GAP,
  );
  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({ height, width })}
      mediaSizeKey={`${width.toString()}x${height.toString()}`}
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({
        onMediaResize,
        onMediaResizeEnd,
        onMediaResizeStart,
        ref,
        style,
      }) => {
        let x = 0;
        return (
          <div
            className="relative shrink-0 select-none"
            ref={ref}
            style={{ ...style, height, width }}
          >
            {entries.map((entry, index) => {
              const size = dimensions[index];
              const left = x;
              x += size.width + PANE_GAP;
              return (
                <div
                  className="absolute"
                  key={entry.trackId}
                  style={{
                    height: size.height,
                    left,
                    top: (height - size.height) / 2,
                    width: size.width,
                  }}
                >
                  <RecordingOutputPane
                    controlsVisible={controlsVisible}
                    entry={entry}
                    isActive={activeTrack === entry.trackId}
                    onBoundsChange={(bounds) => {
                      const base = resizeBaselineRef.current;
                      if (!base) return;
                      // The pane's horizontal origin in the box is the sum of
                      // the preceding pane widths, which this resize does not
                      // touch, so the horizontal shift carries over directly.
                      // Vertically the pane is centered in the box, so the
                      // origin also absorbs the centering delta.
                      const boxHeight = Math.max(
                        bounds.height,
                        base.otherMaxHeight,
                      );
                      onMediaResize({
                        height: boxHeight,
                        originX: bounds.originX,
                        originY:
                          bounds.originY +
                          (base.boxHeight - base.paneHeight) / 2 -
                          (boxHeight - bounds.height) / 2,
                        width: base.boxWidth + bounds.width - base.paneWidth,
                      });
                    }}
                    onCanvasResizeDraft={(settings) => {
                      onCanvasResizeDraft?.(entry.trackId, settings);
                    }}
                    onChange={(settings) => {
                      onChange?.(entry.trackId, settings);
                    }}
                    onMediaResizeEnd={onMediaResizeEnd}
                    onMediaResizeStart={() => {
                      resizeBaselineRef.current = {
                        boxHeight: height,
                        boxWidth: width,
                        otherMaxHeight: dimensions.reduce(
                          (maximum, dimension, other) =>
                            other === index
                              ? maximum
                              : Math.max(maximum, dimension.height),
                          0,
                        ),
                        paneHeight: size.height,
                        paneWidth: size.width,
                      };
                      onMediaResizeStart();
                    }}
                    onSelect={() => {
                      onSelectTrack?.(entry.trackId);
                    }}
                    onTrackContextMenu={(event) => {
                      onTrackContextMenu?.(entry.trackId, event);
                    }}
                    settings={outputs[entry.trackId]}
                    tool={tool}
                  />
                </div>
              );
            })}
          </div>
        );
      }}
      resetKey={`recording-output:${entries.map(({ trackId }) => trackId).join(":")}`}
      zoomPercent={zoomPercent}
    />
  );
}
