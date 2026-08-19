// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useEffect, useRef, useState } from "react";

import { CircularProgressBar } from "../../../components/base/circular-progress-bar/circular-progress-bar";
import { Overlay } from "../../../components/base/overlay/overlay";

/** A pane's geometry is expressed in the unscaled workspace coordinate space. */
type NativeRecordingWorkspacePane = {
  height: number;
  index: number;
  label: string;
  ref: RefObject<HTMLCanvasElement | null>;
  width: number;
  x: number;
  y: number;
};

type NativeRecordingWorkspaceViewportProps = {
  ariaLabel: string;
  isBusy: boolean;
  panes: NativeRecordingWorkspacePane[];
  workspaceHeight: number;
  workspaceWidth: number;
  isSelecting?: boolean;
};

const VIEWPORT_GUTTER = 8;

/**
 * Passive native recording surface.
 *
 * This deliberately has no pointer or wheel handlers. The native compositor
 * owns the panes; the canvases here are marker elements whose layout is
 * measured by useRecordingPreviewSurface.
 */
export function NativeRecordingWorkspaceViewport({
  ariaLabel,
  isBusy,
  isSelecting = false,
  panes,
  workspaceHeight,
  workspaceWidth,
}: NativeRecordingWorkspaceViewportProps) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [viewportSize, setViewportSize] = useState({ height: 0, width: 0 });

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    if (typeof ResizeObserver === "undefined") return;
    const updateSize = (width: number, height: number) => {
      setViewportSize((previous) =>
        previous.width === width && previous.height === height
          ? previous
          : { height, width },
      );
    };
    const observer = new ResizeObserver(([entry]) => {
      updateSize(entry.contentRect.width, entry.contentRect.height);
    });
    observer.observe(viewport);
    return () => {
      observer.disconnect();
    };
  }, []);

  const availableWidth = Math.max(0, viewportSize.width - VIEWPORT_GUTTER * 2);
  const availableHeight = Math.max(
    0,
    viewportSize.height - VIEWPORT_GUTTER * 2,
  );
  const scale =
    workspaceWidth > 0 && workspaceHeight > 0
      ? Math.min(
          availableWidth / workspaceWidth,
          availableHeight / workspaceHeight,
        )
      : 0;
  const fittedWidth = workspaceWidth * scale;
  const fittedHeight = workspaceHeight * scale;

  return (
    <div
      aria-label={ariaLabel}
      className={`relative flex min-h-0 grow overflow-hidden ${isSelecting ? "cursor-move" : ""}`}
      data-recording-preview-viewport
      ref={viewportRef}
      role="img"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 -z-10 bg-black/5 dark:bg-black/25"
        data-preview-backdrop
      />
      <div
        className="absolute shrink-0"
        style={{
          height: `${fittedHeight.toString()}px`,
          left: `${((viewportSize.width - fittedWidth) / 2).toString()}px`,
          top: `${((viewportSize.height - fittedHeight) / 2).toString()}px`,
          width: `${fittedWidth.toString()}px`,
        }}
      >
        {panes.map((pane) => (
          <canvas
            aria-label={pane.label}
            className="pointer-events-none absolute max-w-none opacity-0"
            key={pane.index}
            ref={pane.ref}
            role="img"
            style={{
              height: `${(pane.height * scale).toString()}px`,
              left: `${(pane.x * scale).toString()}px`,
              top: `${(pane.y * scale).toString()}px`,
              width: `${(pane.width * scale).toString()}px`,
            }}
          />
        ))}
      </div>
      <Overlay
        blur="sm"
        className="pointer-events-none"
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
    </div>
  );
}
