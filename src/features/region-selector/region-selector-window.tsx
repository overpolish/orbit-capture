// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { Check, SquareDot } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Rnd } from "react-rnd";

import { Button } from "../../components/base/button/button";
import { AspectRatio } from "../../components/shared/aspect-ratio/aspect-ratio";
import { TransformControls } from "../../components/shared/canvas-tools/transform-controls";
import { CheckOnClickButton } from "../../components/shared/check-on-click-button/check-on-click-button";
import { cn } from "../../lib/styling";
import {
  hideRegionSelector,
  setRecordingControlsOpacity,
  setRegionSelectorOpacity,
  setRegionSelectorPassthrough,
  showRegionSelector,
  takeMonitorScreenshot,
} from "../recording-sources/api";
import { useRecordingSourceStore } from "../recording-sources/store";

import { Magnifier } from "./magnifier";
import { fitRegion, wholePixel, wholePixelSize } from "./region-geometry";
import { HANDLE_CLASSES, HANDLE_STYLES } from "./resize-handles";
import { ResizeDirection } from "./types";

export function RegionSelectorWindow() {
  const {
    isRegionEditing,
    recordingMode,
    region,
    selectedMonitor,
    setRegion,
    setRegionEditing,
  } = useRecordingSourceStore((state) => state);
  const [draft, setDraft] = useState(region);
  const [activeAspect, setActiveAspect] = useState<number>();
  const [resizeDirection, setResizeDirection] = useState<ResizeDirection>();
  const [isDragging, setIsDragging] = useState(false);
  const [screenshot, setScreenshot] = useState<ArrayBuffer | null>(null);
  const activeHandleRef = useRef<HTMLElement | null>(null);

  const persistDraft = useCallback(() => {
    setRegion({
      position: {
        x: wholePixel(draft.position.x),
        y: wholePixel(draft.position.y),
      },
      size: {
        height: wholePixelSize(draft.size.height),
        width: wholePixelSize(draft.size.width),
      },
    });
  }, [draft, setRegion]);

  useEffect(() => {
    // Cross-window storage updates replace the persisted region.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(region);
  }, [region]);

  useEffect(() => {
    if (recordingMode !== "region" || !selectedMonitor) {
      void hideRegionSelector();
      return;
    }

    const fitted = fitRegion(
      region,
      selectedMonitor.size.width,
      selectedMonitor.size.height,
    );
    // The overlay keeps a local draft so dragging does not write storage per frame.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(fitted);
    if (JSON.stringify(fitted) !== JSON.stringify(region)) setRegion(fitted);
    void showRegionSelector(selectedMonitor);
  }, [recordingMode, region, selectedMonitor, setRegion]);

  useEffect(() => {
    if (recordingMode !== "region" || !selectedMonitor) return;

    void setRegionSelectorPassthrough(!isRegionEditing);
    void setRecordingControlsOpacity(isRegionEditing ? 0 : 1);
    if (!isRegionEditing) return;

    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = setScreenshot;
    // Clear the prior monitor image before asynchronously capturing the next one.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setScreenshot(null);
    void setRegionSelectorOpacity(0).then(async () => {
      try {
        await takeMonitorScreenshot(selectedMonitor.id, channel);
      } finally {
        await setRegionSelectorOpacity(1);
      }
    });
  }, [isRegionEditing, recordingMode, selectedMonitor]);

  const center = () => {
    if (!selectedMonitor) return;
    const centered = {
      ...draft,
      position: {
        x: wholePixel((selectedMonitor.size.width - draft.size.width) / 2),
        y: wholePixel((selectedMonitor.size.height - draft.size.height) / 2),
      },
    };
    setDraft(centered);
    setRegion(centered);
  };

  if (recordingMode !== "region" || !selectedMonitor) return null;

  const showActions = isRegionEditing && !resizeDirection && !isDragging;
  const isMac = navigator.userAgent.includes("Mac");

  return (
    <main
      className={cn(
        "relative h-screen w-screen overflow-hidden select-none",
        resizeDirection && "cursor-none [&_*]:cursor-none!",
      )}
    >
      <svg aria-hidden className="pointer-events-none absolute size-full">
        <defs>
          <mask id="region-cutout">
            <rect className="fill-white" height="100%" width="100%" />
            <rect
              className="fill-black"
              height={draft.size.height}
              width={draft.size.width}
              x={draft.position.x}
              y={draft.position.y}
            />
          </mask>
        </defs>
        <rect
          className="fill-black/50"
          height="100%"
          mask="url(#region-cutout)"
          width="100%"
        />
      </svg>

      <Rnd
        bounds="parent"
        className={cn(
          "relative transition-opacity",
          !isRegionEditing && "invisible opacity-0",
        )}
        dragGrid={[1, 1]}
        lockAspectRatio={activeAspect ?? false}
        onDrag={(_event, data) => {
          setDraft((current) => ({
            ...current,
            position: { x: data.x, y: data.y },
          }));
        }}
        onDragStart={() => {
          setIsDragging(true);
        }}
        onDragStop={() => {
          persistDraft();
          setIsDragging(false);
        }}
        // react-rnd defines this callback with five required parameters.
        // eslint-disable-next-line @typescript-eslint/max-params
        onResize={(_event, _direction, element, _delta, position) => {
          setDraft({
            position,
            size: {
              height: Number.parseInt(element.style.height, 10),
              width: Number.parseInt(element.style.width, 10),
            },
          });
        }}
        onResizeStart={(_event, direction, element) => {
          activeHandleRef.current = element.querySelector(
            `.${HANDLE_CLASSES[direction] ?? ""}`,
          );
          setResizeDirection(direction);
        }}
        onResizeStop={() => {
          persistDraft();
          activeHandleRef.current = null;
          setResizeDirection(undefined);
        }}
        position={draft.position}
        resizeGrid={[1, 1]}
        resizeHandleClasses={HANDLE_CLASSES}
        resizeHandleStyles={HANDLE_STYLES}
        size={draft.size}
      >
        {/* The same marquee chrome as the export window's crop controls;
            react-rnd supplies behaviour through its own invisible handles. */}
        <TransformControls
          frame={{
            height: draft.size.height,
            width: draft.size.width,
            x: 0,
            y: 0,
          }}
          inverseScale="1"
        />
      </Rnd>

      <div
        className={cn(
          "absolute left-1/2 flex -translate-x-1/2 items-center justify-center opacity-0 transition-opacity",
          isMac ? "top-12" : "top-2",
          showActions && "opacity-100",
        )}
      >
        <div
          className={cn(
            "pointer-events-none flex items-center gap-2 rounded-md border border-muted/25 bg-content p-2 shadow-md",
            showActions && "pointer-events-auto",
          )}
        >
          <CheckOnClickButton
            onPress={center}
            showFocus={false}
            size="sm"
            variant="ghost"
          >
            <SquareDot aria-hidden size={14} />
            Center
          </CheckOnClickButton>
          <AspectRatio
            height={draft.size.height}
            onRatioChange={setActiveAspect}
            setHeight={(height) => {
              setDraft((current) => ({
                ...current,
                size: { ...current.size, height: wholePixelSize(height) },
              }));
            }}
            setWidth={(width) => {
              setDraft((current) => ({
                ...current,
                size: { ...current.size, width: wholePixelSize(width) },
              }));
            }}
            width={draft.size.width}
          />
          <Button
            color="success"
            onPress={() => {
              persistDraft();
              setRegionEditing(false);
            }}
            showFocus={false}
            size="sm"
          >
            <Check aria-hidden size={18} />
            Finish
          </Button>
        </div>
      </div>

      {screenshot ? (
        <Magnifier
          monitor={selectedMonitor}
          regionRect={{
            height: draft.size.height,
            width: draft.size.width,
            x: draft.position.x,
            y: draft.position.y,
          }}
          resizeDirection={resizeDirection}
          screenshot={screenshot}
        />
      ) : null}
    </main>
  );
}
