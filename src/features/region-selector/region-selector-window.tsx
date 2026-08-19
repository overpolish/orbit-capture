// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { Check, ImageDown, SquareDot } from "lucide-react";
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
  setRegionSelectorPassthrough,
  showRegionSelector,
  takeMonitorScreenshot,
} from "../recording-sources/api";
import { useRecordingSourceStore } from "../recording-sources/store";
import { Region } from "../recording-sources/types";
import { ShortcutAction } from "../settings/types";
import { useGeneralSettings } from "../settings/use-general-settings";

import { Magnifier } from "./magnifier";
import { RegionDrawingSurface } from "./region-drawing-surface";
import {
  EMPTY_REGION,
  fitRegion,
  hasRegion,
  snapRegion,
  wholePixel,
  wholePixelSize,
} from "./region-geometry";
import { HANDLE_CLASSES, HANDLE_STYLES } from "./resize-handles";
import {
  beginScreenshotCapture,
  captureScreenshotRegion,
  endScreenshotCapture,
} from "./screenshot-session";
import { ResizeDirection } from "./types";
import { useKeyHeld } from "./use-key-held";

const SHORTCUT_ACTION_EVENT = "global-shortcut://action";
// Holding this ignores the linked ratio, so the region can be reshaped
// freely; the shape it ends up with becomes the new ratio.
const FREE_ASPECT_KEY = "Shift";

export function RegionSelectorWindow() {
  const {
    isRegionEditing,
    isScreenshotCapture,
    recordingMode,
    region,
    selectedMonitor,
    setRegion,
    setRegionEditing,
  } = useRecordingSourceStore((state) => state);
  const [draft, setDraft] = useState(region);
  const [seededForSession, setSeededForSession] = useState(isScreenshotCapture);
  if (seededForSession !== isScreenshotCapture) {
    // Reseed while rendering rather than in an effect: the session flag and
    // the editing flag flip together, so an effect would leave one painted
    // frame of the marquee sitting on the recording region before the empty
    // draft landed - a visible flash when the overlay was already on screen
    // for region mode.
    setSeededForSession(isScreenshotCapture);
    setDraft(isScreenshotCapture ? EMPTY_REGION : region);
  }
  const [activeAspect, setActiveAspect] = useState<number>();
  const [resizeDirection, setResizeDirection] = useState<ResizeDirection>();
  const [isDragging, setIsDragging] = useState(false);
  const [isDrawing, setIsDrawing] = useState(false);
  const [screenshot, setScreenshot] = useState<ArrayBuffer | null>(null);
  const [freedThisResize, setFreedThisResize] = useState(false);
  const freeAspect = useKeyHeld(FREE_ASPECT_KEY);
  const activeHandleRef = useRef<HTMLElement | null>(null);
  // A capture ends the session, so whichever gesture starts one shuts the
  // others out until the session is over.
  const isCapturingRef = useRef(false);
  const generalSettings = useGeneralSettings();
  const captureOnDraw = generalSettings?.captureScreenshotOnDraw ?? false;

  // The overlay is the screenshot shortcut's own surface as well as the
  // recording region's, so it shows for either reason.
  const activeMonitor =
    recordingMode === "region" || isScreenshotCapture ? selectedMonitor : null;

  const persistDraft = useCallback((): Region => {
    const persisted = snapRegion(draft);
    if (!isScreenshotCapture) setRegion(persisted);

    return persisted;
  }, [draft, isScreenshotCapture, setRegion]);

  useEffect(() => {
    // Cross-window storage updates replace the persisted region. A screenshot
    // session starts from nothing instead: the region is drawn for that shot
    // alone, and the persisted one comes back when the session ends.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(isScreenshotCapture ? EMPTY_REGION : region);
  }, [isScreenshotCapture, region]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    // Emitting to a window does not scope delivery: `listen` registers for any
    // target, so every window sees every shortcut action and each listener has
    // to match the one it owns exactly.
    void listen<ShortcutAction>(SHORTCUT_ACTION_EVENT, ({ payload }) => {
      if (payload !== "takeScreenshot") return;
      beginScreenshotCapture().catch((error: unknown) => {
        console.error("Could not open the region for a screenshot", error);
      });
    }).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    // Each session gets its one capture back.
    isCapturingRef.current = false;
    if (!isScreenshotCapture) return;

    const cancel = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      void endScreenshotCapture();
    };

    window.addEventListener("keydown", cancel);

    return () => {
      window.removeEventListener("keydown", cancel);
    };
  }, [isScreenshotCapture]);

  useEffect(() => {
    if (!activeMonitor) {
      void hideRegionSelector();
      return;
    }

    const fitted = fitRegion(
      region,
      activeMonitor.size.width,
      activeMonitor.size.height,
    );
    // The overlay keeps a local draft so dragging does not write storage per
    // frame. A screenshot session has no region until one is drawn, so its
    // draft starts empty rather than from the recording region.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setDraft(isScreenshotCapture ? EMPTY_REGION : fitted);
    if (
      !isScreenshotCapture &&
      JSON.stringify(fitted) !== JSON.stringify(region)
    ) {
      setRegion(fitted);
    }
    void showRegionSelector(activeMonitor);
  }, [activeMonitor, isScreenshotCapture, region, setRegion]);

  useEffect(() => {
    if (!activeMonitor) return;

    void setRegionSelectorPassthrough(!isRegionEditing);
    void setRecordingControlsOpacity(isRegionEditing ? 0 : 1);
    if (!isRegionEditing) return;

    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = setScreenshot;
    // Clear the prior monitor image before asynchronously capturing the next
    // one. The capture leaves Screenwide's windows out, so the overlay stays
    // exactly as it is while the image is taken.
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setScreenshot(null);
    void takeMonitorScreenshot(activeMonitor.id, channel);
  }, [activeMonitor, isRegionEditing]);

  // Until a region is drawn there is nothing to move, resize, centre or
  // capture, which a screenshot session starts out with.
  const regionPlaced = hasRegion(draft);

  const center = () => {
    if (!activeMonitor || !regionPlaced) return;
    const centered = {
      ...draft,
      position: {
        x: wholePixel((activeMonitor.size.width - draft.size.width) / 2),
        y: wholePixel((activeMonitor.size.height - draft.size.height) / 2),
      },
    };
    setDraft(centered);
    if (!isScreenshotCapture) setRegion(centered);
  };

  const finish = useCallback(() => {
    if (!activeMonitor) return;
    if (isScreenshotCapture) {
      if (isCapturingRef.current) return;
      isCapturingRef.current = true;
      captureScreenshotRegion(activeMonitor.id, persistDraft());
      return;
    }
    persistDraft();
    setRegionEditing(false);
  }, [activeMonitor, isScreenshotCapture, persistDraft, setRegionEditing]);

  // The toolbar is up for the whole of an edit, region or not: an aspect
  // preset picked before anything is drawn is the ratio to draw at. Instant
  // capture has no step after the draw for it to serve, so it stays away.
  const showActions =
    isRegionEditing &&
    !resizeDirection &&
    !isDragging &&
    !isDrawing &&
    !(isScreenshotCapture && captureOnDraw);
  const canFinish = showActions && regionPlaced;

  useEffect(() => {
    // re-resizable fixes the ratio when a resize starts, so handing it a
    // number again mid-gesture snaps the region back to the shape it began
    // with. Freeing once therefore has to hold until the gesture ends.
    if (!freeAspect || !resizeDirection) return;
    // eslint-disable-next-line @eslint-react/set-state-in-effect
    setFreedThisResize(true);
  }, [freeAspect, resizeDirection]);

  useEffect(() => {
    if (!canFinish) return;

    const finishOnEnter = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || event.repeat || event.isComposing) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      finish();
    };

    window.addEventListener("keydown", finishOnEnter, true);
    return () => {
      window.removeEventListener("keydown", finishOnEnter, true);
    };
  }, [canFinish, finish]);

  if (!activeMonitor) return null;

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

      <RegionDrawingSurface
        aspect={freeAspect ? undefined : activeAspect}
        bounds={activeMonitor.size}
        current={draft}
        isEditing={isRegionEditing}
        onChange={setDraft}
        onDrawingChange={setIsDrawing}
        onFinish={(nextRegion) => {
          if (!isScreenshotCapture) {
            setRegion(nextRegion);
            return;
          }
          // Releasing the region is the whole gesture when instant capture is
          // on: the region just drawn goes straight to the shot, since `draft`
          // may not have re-rendered with it yet.
          if (!captureOnDraw || isCapturingRef.current) return;
          isCapturingRef.current = true;
          captureScreenshotRegion(activeMonitor.id, snapRegion(nextRegion));
        }}
      />

      <Rnd
        bounds="parent"
        className={cn(
          "relative transition-opacity",
          (!isRegionEditing || !regionPlaced) && "invisible opacity-0",
        )}
        dragGrid={[1, 1]}
        lockAspectRatio={
          freeAspect || freedThisResize ? false : (activeAspect ?? false)
        }
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
          setFreedThisResize(false);
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
            isDisabled={!regionPlaced}
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
              // A ratio picked before a region exists is only the shape to
              // draw at; there is nothing to resize yet.
              if (!regionPlaced) return;
              setDraft((current) => ({
                ...current,
                size: { ...current.size, height: wholePixelSize(height) },
              }));
            }}
            setWidth={(width) => {
              // As above: no region, nothing to resize.
              if (!regionPlaced) return;
              setDraft((current) => ({
                ...current,
                size: { ...current.size, width: wholePixelSize(width) },
              }));
            }}
            width={draft.size.width}
          />
          <Button
            color="success"
            isDisabled={!regionPlaced}
            onPress={finish}
            showFocus={false}
            size="sm"
          >
            {isScreenshotCapture ? (
              <ImageDown aria-hidden size={18} />
            ) : (
              <Check aria-hidden size={18} />
            )}
            {isScreenshotCapture ? "Capture" : "Finish"}
          </Button>
        </div>
      </div>

      {screenshot ? (
        <Magnifier
          monitor={activeMonitor}
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
