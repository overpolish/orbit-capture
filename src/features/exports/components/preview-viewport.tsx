// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useRef } from "react";

import {
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";
import { usePreviewCapabilities } from "../use-preview-capabilities";
import { useScreenshotPreviewSurface } from "../use-screenshot-preview-surface";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { NativeOutputPreview } from "./native-output-preview";
import { ScreenshotPreviewLayer } from "./screenshot-preview-layer";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

type PreviewViewportProps = {
  alt: string;
  artifactId: number;
  naturalHeight: number;
  naturalWidth: number;
  isEditing?: boolean;
  onBackgroundRadiusChange?: (radiusPercent: number) => void;
  onBackgroundRadiusChangeEnd?: () => void;
  onNeedFullResolution?: () => void;
  onOutputChange?: (settings: ScreenshotOutputSettings) => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  previewUrl?: string | null;
  radiusPercent?: number;
  screenshotOutput?: ScreenshotOutputSettings;
  zoomPercent?: number;
};

export function PreviewViewport({
  alt,
  artifactId,
  isEditing = false,
  naturalHeight,
  naturalWidth,
  onBackgroundRadiusChange,
  onBackgroundRadiusChangeEnd,
  onNeedFullResolution,
  onOutputChange,
  onRadiusChange,
  onRadiusChangeEnd,
  onZoomChange,
  previewUrl,
  radiusPercent = 0,
  screenshotOutput,
  zoomPercent,
}: PreviewViewportProps) {
  const outputRef = useRef<HTMLDivElement | null>(null);
  const composedCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const output = screenshotOutput
    ? screenshotOutputDimensions(screenshotOutput)
    : { height: naturalHeight, width: naturalWidth };
  // `undefined` until the capability probe resolves: neither preview path is
  // rendered before then, so the viewport never flashes the wrong one.
  const nativePane = usePreviewCapabilities()?.nativeScreenshotPreview;
  useScreenshotPreviewSurface({
    artifactId,
    canvasRef: composedCanvasRef,
    isEnabled: nativePane === true && screenshotOutput !== undefined,
    output: screenshotOutput,
  });
  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => output}
      hideUntilMeasured
      onNeedFullResolution={onNeedFullResolution}
      onZoomChange={onZoomChange}
      renderMedia={({ onReady, ref, style }) => (
        <div
          className="absolute shrink-0 select-none"
          ref={(element) => {
            outputRef.current = element;
            ref(element);
          }}
          style={{
            ...style,
            height: `${output.height.toString()}px`,
            left: `calc(50% - ${(output.width / 2).toString()}px)`,
            top: `calc(50% - ${(output.height / 2).toString()}px)`,
            width: `${output.width.toString()}px`,
          }}
        >
          <div className="absolute inset-0 overflow-visible bg-transparent">
            {screenshotOutput && nativePane === true ? (
              // A geometry marker: the composed screenshot renders on the
              // native pane surface below the webview, positioned to this
              // element's rect by the layout hook.
              <canvas
                aria-label="Composed output preview"
                className="absolute inset-0 size-full max-w-none opacity-0"
                ref={composedCanvasRef}
                role="img"
              />
            ) : screenshotOutput && nativePane === false ? (
              <NativeOutputPreview
                artifactId={artifactId}
                canvasRef={composedCanvasRef}
                onReady={onReady}
                output={screenshotOutput}
              />
            ) : previewUrl && screenshotOutput === undefined ? (
              <img
                alt={alt}
                className="absolute inset-0 size-full"
                draggable={false}
                onLoad={onReady}
                src={previewUrl}
              />
            ) : null}
          </div>
          {screenshotOutput ? (
            <ScreenshotPreviewLayer
              isEditing={isEditing}
              onOutputChange={onOutputChange}
              onRadiusChange={onRadiusChange}
              onRadiusChangeEnd={onRadiusChangeEnd}
              output={output}
              outputRef={outputRef}
              previewUrl={previewUrl}
              radiusPercent={radiusPercent}
              settings={screenshotOutput}
              source={{ height: naturalHeight, width: naturalWidth }}
            />
          ) : null}
          {screenshotOutput && isEditing ? (
            <ScreenshotRadiusControl
              anchor="top-right"
              height={output.height}
              mediaRef={outputRef}
              onChange={onBackgroundRadiusChange}
              onChangeEnd={onBackgroundRadiusChangeEnd}
              radiusPercent={screenshotOutput.backgroundRadiusPercent}
              width={output.width}
            />
          ) : null}
        </div>
      )}
      resetKey={`${artifactId.toString()}:${previewUrl ?? "empty"}:${output.width.toString()}:${output.height.toString()}`}
      zoomPercent={zoomPercent}
    />
  );
}
