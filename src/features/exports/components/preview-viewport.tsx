// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ImageIcon } from "lucide-react";
import { useRef } from "react";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";
import { ScreenshotRadiusControl } from "./screenshot-radius-control";

type PreviewViewportProps = {
  alt: string;
  artifactId: number;
  naturalHeight: number;
  naturalWidth: number;
  onNeedFullResolution?: () => void;
  onRadiusChange?: (radiusPercent: number) => void;
  onRadiusChangeEnd?: () => void;
  previewUrl?: string | null;
  radiusPercent?: number;
};

export function PreviewViewport({
  alt,
  artifactId,
  naturalHeight,
  naturalWidth,
  onNeedFullResolution,
  onRadiusChange,
  onRadiusChangeEnd,
  previewUrl,
  radiusPercent = 0,
}: PreviewViewportProps) {
  const mediaRef = useRef<HTMLDivElement | null>(null);
  const radius = (Math.min(naturalWidth, naturalHeight) * radiusPercent) / 100;
  return (
    <InteractivePreviewViewport<HTMLDivElement>
      getMediaSize={() => ({ height: naturalHeight, width: naturalWidth })}
      hideUntilMeasured
      onNeedFullResolution={onNeedFullResolution}
      renderMedia={({ onReady, ref, style }) => (
        <div
          className="absolute flex shrink-0 items-center justify-center overflow-hidden select-none"
          ref={(element) => {
            mediaRef.current = element;
            ref(element);
          }}
          style={{
            ...style,
            borderRadius: `${radius.toString()}px`,
            height: `${naturalHeight.toString()}px`,
            left: `calc(50% - ${(naturalWidth / 2).toString()}px)`,
            top: `calc(50% - ${(naturalHeight / 2).toString()}px)`,
            width: `${naturalWidth.toString()}px`,
          }}
        >
          {previewUrl ? (
            <img
              alt={alt}
              className="absolute inset-0 size-full max-w-none"
              draggable={false}
              onLoad={onReady}
              src={previewUrl}
            />
          ) : (
            <ImageIcon className="text-muted/50" size={40} />
          )}
          <ScreenshotRadiusControl
            height={naturalHeight}
            mediaRef={mediaRef}
            onChange={onRadiusChange}
            onChangeEnd={onRadiusChangeEnd}
            radiusPercent={radiusPercent}
            width={naturalWidth}
          />
        </div>
      )}
      resetKey={`${artifactId.toString()}:${previewUrl ?? "empty"}`}
    />
  );
}
