// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject } from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";

import { InteractivePreviewViewport } from "./interactive-preview-viewport";

export function CanvasPreviewViewport({
  canvasRef,
  height,
  isBusy,
  label,
  onNeedFullResolution,
  width,
}: {
  canvasRef: RefObject<HTMLCanvasElement | null>;
  height: number;
  isBusy: boolean;
  label: string;
  width: number;
  onNeedFullResolution?: () => void;
}) {
  return (
    <InteractivePreviewViewport<HTMLCanvasElement>
      getMediaSize={() => ({ height, width })}
      onNeedFullResolution={onNeedFullResolution}
      renderMedia={({ ref, style }) => (
        <>
          <canvas
            aria-label={label}
            className="max-w-none shrink-0 select-none"
            ref={(element) => {
              canvasRef.current = element;
              ref(element);
            }}
            role="img"
            style={{
              ...style,
              height: `${height.toString()}px`,
              width: `${width.toString()}px`,
            }}
          />
          <Overlay
            blur="sm"
            className="pointer-events-none rounded-md"
            contained
            isOpen={isBusy}
          >
            <CircularProgressBar
              aria-label="Preparing the preview"
              isIndeterminate
              size={32}
              strokeWidth={10}
            />
          </Overlay>
        </>
      )}
      resetKey={`${label}:${width.toString()}x${height.toString()}`}
    />
  );
}
