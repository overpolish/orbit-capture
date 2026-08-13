// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useRef } from "react";

import {
  RecordingOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";
import { RecordingPreviewPane, RecordingVideoTrackId } from "../types";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
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
  onChange,
  settings,
}: {
  controlsVisible: boolean;
  entry: Entry;
  settings: RecordingOutputSettings[RecordingVideoTrackId];
  onChange?: (settings: RecordingOutputSettings[RecordingVideoTrackId]) => void;
}) {
  const outputRef = useRef<HTMLDivElement | null>(null);
  const output = screenshotOutputDimensions(settings);
  const source = {
    height: entry.pane.sourceHeight,
    width: entry.pane.sourceWidth,
  };
  return (
    <div
      className="absolute select-none"
      ref={outputRef}
      style={{ height: output.height, width: output.width }}
    >
      <canvas
        aria-label={`${entry.pane.kind === "camera" ? "Camera" : "Screen"} composed preview`}
        className="absolute inset-0 size-full max-w-none opacity-0"
        ref={entry.canvasRef}
        role="img"
      />
      <ScreenshotPreviewLayer
        isEditing={controlsVisible}
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
    </div>
  );
}

export function RecordingOutputPreviewViewport({
  activeTrack,
  entries,
  isEditing,
  onChange,
  onNeedFullResolution,
  onZoomChange,
  outputs,
  zoomPercent,
}: {
  activeTrack: RecordingVideoTrackId | null;
  entries: Entry[];
  isEditing: boolean;
  outputs: RecordingOutputSettings;
  onChange?: (
    trackId: RecordingVideoTrackId,
    settings: RecordingOutputSettings[RecordingVideoTrackId],
  ) => void;
  onNeedFullResolution?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  zoomPercent?: number;
}) {
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
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({ ref, style }) => {
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
                    controlsVisible={isEditing && activeTrack === entry.trackId}
                    entry={entry}
                    onChange={(settings) => {
                      onChange?.(entry.trackId, settings);
                    }}
                    settings={outputs[entry.trackId]}
                  />
                </div>
              );
            })}
          </div>
        );
      }}
      resetKey={`recording-output:${entries.map(({ trackId }) => `${trackId}-${outputs[trackId].width.toString()}x${outputs[trackId].height.toString()}`).join(":")}`}
      zoomPercent={zoomPercent}
    />
  );
}
