// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useRef } from "react";

import {
  ScreenshotOutputSettings,
  screenshotOutputDimensions,
} from "../screenshot-output";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { MeshBackgroundPreview } from "./mesh-background-preview";
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
  const output = screenshotOutput
    ? screenshotOutputDimensions(screenshotOutput)
    : { height: naturalHeight, width: naturalWidth };
  const outputRadius = screenshotOutput
    ? (Math.min(output.width, output.height) *
        screenshotOutput.backgroundRadiusPercent) /
      100
    : 0;
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
          <div
            className="absolute inset-0 overflow-hidden bg-transparent"
            style={{
              background:
                screenshotOutput?.backgroundType === "solid"
                  ? screenshotOutput.backgroundColor
                  : "transparent",
              clipPath: `inset(0 round ${outputRadius.toString()}px)`,
            }}
          >
            {screenshotOutput?.backgroundType === "mesh" ? (
              <MeshBackgroundPreview
                colors={screenshotOutput.meshColors}
                points={screenshotOutput.meshPoints}
                seed={screenshotOutput.meshSeed}
                size={output}
                warpPercent={screenshotOutput.meshWarpPercent}
              />
            ) : null}
            {!screenshotOutput && previewUrl ? (
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
              alt={alt}
              canvasRadius={outputRadius}
              isEditing={isEditing}
              onOutputChange={onOutputChange}
              onRadiusChange={onRadiusChange}
              onRadiusChangeEnd={onRadiusChangeEnd}
              onReady={onReady}
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
