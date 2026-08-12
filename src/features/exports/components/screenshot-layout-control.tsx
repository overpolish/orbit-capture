// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { AnimatePresence } from "motion/react";
import {
  PointerEvent as ReactPointerEvent,
  RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import { flushSync } from "react-dom";

import { TransformControls } from "../../../components/shared/canvas-tools/transform-controls";
import { clamp, snapCameraFramePosition } from "../camera-overlay-geometry";
import {
  ScreenshotOutputSettings,
  screenshotLayout,
} from "../screenshot-output";

import { ScreenshotCropMagnifier } from "./screenshot-crop-magnifier";
import {
  constrainedHandlePoint,
  cropEdgesInsideImage,
  radialResizeCursor,
  ScreenshotLayoutEdge as Edge,
} from "./screenshot-layout-geometry";

type Action =
  | { kind: "crop" | "whole"; offsetX: number; offsetY: number }
  | { edges: Edge[]; kind: "resize" }
  | { kind: "scale" };

export function ScreenshotLayoutControl({
  mediaRef,
  onChange,
  onChangeEnd,
  output,
  previewUrl,
  settings,
  source,
}: {
  mediaRef: RefObject<HTMLDivElement | null>;
  output: { height: number; width: number };
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  onChange?: (settings: ScreenshotOutputSettings) => void;
  onChangeEnd?: (settings: ScreenshotOutputSettings) => void;
  previewUrl?: string | null;
}) {
  const activeRef = useRef<{
    action: Action;
    settings: ScreenshotOutputSettings;
  } | null>(null);
  const [activeEdges, setActiveEdges] = useState<Edge[] | null>(null);
  const [magnifierPoint, setMagnifierPoint] = useState({ x: 0, y: 0 });
  const [ringCursor, setRingCursor] = useState("nesw-resize");
  const [draft, setDraft] = useState(settings);
  const draftRef = useRef(settings);
  const imageRef = useRef<HTMLImageElement | null>(null);
  useEffect(() => {
    if (!activeRef.current) {
      draftRef.current = settings;
      setDraft(settings);
    }
  }, [settings]);
  useEffect(() => {
    if (!previewUrl) {
      imageRef.current = null;
      return;
    }
    const image = new Image();
    image.onload = () => {
      imageRef.current = image;
    };
    image.src = previewUrl;
  }, [previewUrl]);
  const layout = screenshotLayout(source, output, draft);
  const inverseScale = "var(--preview-inverse-scale, 1)";
  const ringExtent = Math.min(layout.crop.width, layout.crop.height) * 0.38;

  const naturalPoint = (event: ReactPointerEvent) => {
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return null;
    return {
      x: ((event.clientX - bounds.left) * output.width) / bounds.width,
      y: ((event.clientY - bounds.top) * output.height) / bounds.height,
    };
  };
  const begin = (event: ReactPointerEvent, action: Action) => {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    activeRef.current = { action, settings: draftRef.current };
    setActiveEdges(action.kind === "resize" ? action.edges : null);
    const point = naturalPoint(event);
    if (point)
      setMagnifierPoint(
        action.kind === "resize"
          ? constrainedHandlePoint(layout.crop, action.edges, point)
          : point,
      );
  };
  const move = (event: ReactPointerEvent) => {
    const active = activeRef.current;
    const point = naturalPoint(event);
    if (!active || !point) return;
    event.preventDefault();
    event.stopPropagation();
    const start = screenshotLayout(source, output, active.settings);
    const next = { ...active.settings };
    if (active.action.kind === "crop" || active.action.kind === "whole") {
      const rawX = point.x - active.action.offsetX;
      const rawY = point.y - active.action.offsetY;
      let x =
        active.action.kind === "crop"
          ? clamp(
              rawX,
              start.image.x,
              start.image.x + start.image.width - start.crop.width,
            )
          : rawX;
      let y =
        active.action.kind === "crop"
          ? clamp(
              rawY,
              start.image.y,
              start.image.y + start.image.height - start.crop.height,
            )
          : rawY;
      if (active.action.kind === "whole" && (event.metaKey || event.ctrlKey)) {
        const snapped = snapCameraFramePosition({
          frame: start.crop,
          position: { x, y },
          screen: output,
        });
        x = snapped.x;
        y = snapped.y;
      }
      const deltaX = x - start.crop.x;
      const deltaY = y - start.crop.y;
      next.screenshotCropXPercent = (x * 100) / output.width;
      next.screenshotCropYPercent = (y * 100) / output.height;
      if (active.action.kind === "whole") {
        next.screenshotImageXPercent += (deltaX * 100) / output.width;
        next.screenshotImageYPercent += (deltaY * 100) / output.height;
      }
    } else if (active.action.kind === "resize") {
      const minimum = Math.max(
        2,
        (36 * output.width) / (mediaRef.current?.clientWidth || 1),
      );
      const bounds = cropEdgesInsideImage(start.image, start.crop, minimum);
      let { bottom, left, right, top } = bounds;
      if (active.action.edges.includes("left"))
        left = clamp(point.x, bounds.imageLeft, right - bounds.minimumWidth);
      if (active.action.edges.includes("right"))
        right = clamp(point.x, left + bounds.minimumWidth, bounds.imageRight);
      if (active.action.edges.includes("top"))
        top = clamp(point.y, bounds.imageTop, bottom - bounds.minimumHeight);
      if (active.action.edges.includes("bottom"))
        bottom = clamp(point.y, top + bounds.minimumHeight, bounds.imageBottom);
      const crop = {
        height: bottom - top,
        width: right - left,
        x: left,
        y: top,
      };
      next.screenshotCropHeightPercent = (crop.height * 100) / output.height;
      next.screenshotCropWidthPercent = (crop.width * 100) / output.width;
      next.screenshotCropXPercent = (crop.x * 100) / output.width;
      next.screenshotCropYPercent = (crop.y * 100) / output.height;
      setMagnifierPoint(
        constrainedHandlePoint(crop, active.action.edges, point),
      );
    } else {
      const centerX = start.crop.x + start.crop.width / 2;
      const centerY = start.crop.y + start.crop.height / 2;
      const extent = Math.hypot(point.x - centerX, point.y - centerY);
      const baseExtent = Math.max(
        1,
        Math.min(start.crop.width, start.crop.height) * 0.38,
      );
      const minimum = Math.max(
        2,
        (36 * output.width) / (mediaRef.current?.clientWidth || 1),
      );
      const scale = clamp(extent / baseExtent, minimum / start.crop.width, 8);
      const crop = {
        height: start.crop.height * scale,
        width: start.crop.width * scale,
        x: centerX - (start.crop.width * scale) / 2,
        y: centerY - (start.crop.height * scale) / 2,
      };
      const imageCenterX = start.image.x + start.image.width / 2;
      const imageCenterY = start.image.y + start.image.height / 2;
      const image = {
        height: start.image.height * scale,
        width: start.image.width * scale,
        x:
          centerX +
          (imageCenterX - centerX) * scale -
          (start.image.width * scale) / 2,
        y:
          centerY +
          (imageCenterY - centerY) * scale -
          (start.image.height * scale) / 2,
      };
      next.screenshotCropWidthPercent = (crop.width * 100) / output.width;
      next.screenshotCropHeightPercent = (crop.height * 100) / output.height;
      next.screenshotCropXPercent = (crop.x * 100) / output.width;
      next.screenshotCropYPercent = (crop.y * 100) / output.height;
      next.screenshotImageWidthPercent = (image.width * 100) / output.width;
      next.screenshotImageXPercent =
        ((image.x + image.width / 2) * 100) / output.width;
      next.screenshotImageYPercent =
        ((image.y + image.height / 2) * 100) / output.height;
    }
    draftRef.current = next;
    // eslint-disable-next-line @eslint-react/dom-no-flush-sync
    flushSync(() => {
      setDraft(next);
    });
    onChange?.(next);
  };
  const finish = (event: ReactPointerEvent) => {
    event.stopPropagation();
    onChangeEnd?.(draftRef.current);
    activeRef.current = null;
    setActiveEdges(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    if (
      event.currentTarget instanceof HTMLElement ||
      event.currentTarget instanceof SVGElement
    )
      event.currentTarget.blur();
  };
  const interaction = {
    onPointerCancel: finish,
    onPointerMove: move,
    onPointerUp: finish,
  };

  return (
    <div
      className="pointer-events-none absolute touch-none"
      style={{
        height: layout.crop.height,
        left: layout.crop.x,
        top: layout.crop.y,
        width: layout.crop.width,
      }}
    >
      <div
        className="pointer-events-auto absolute inset-0 cursor-move"
        onPointerDown={(event) => {
          const point = naturalPoint(event);
          if (point)
            begin(event, {
              kind: "whole",
              offsetX: point.x - layout.crop.x,
              offsetY: point.y - layout.crop.y,
            });
        }}
        {...interaction}
      />
      <TransformControls
        frame={{
          height: layout.crop.height,
          width: layout.crop.width,
          x: 0,
          y: 0,
        }}
        interaction={interaction}
        inverseScale={inverseScale}
        move={{
          label: "Move screenshot crop",
          onPointerDown: (event) => {
            const point = naturalPoint(event);
            if (point)
              begin(event, {
                kind: "crop",
                offsetX: point.x - layout.crop.x,
                offsetY: point.y - layout.crop.y,
              });
          },
        }}
        resize={{
          label: (edges) => `Crop screenshot ${edges.join(" ")}`,
          onPointerDown: (edges) => (event) => {
            begin(event, { edges, kind: "resize" });
          },
        }}
        scaleRing={{
          cursor: ringCursor,
          extent: ringExtent,
          label: "Scale screenshot",
          onPointerDown: (event) => {
            const point = naturalPoint(event);
            if (point)
              setRingCursor(
                radialResizeCursor(point, {
                  x: layout.crop.x + layout.crop.width / 2,
                  y: layout.crop.y + layout.crop.height / 2,
                }),
              );
            begin(event, { kind: "scale" });
          },
          onPointerMove: (event) => {
            const point = naturalPoint(event);
            if (point)
              setRingCursor(
                radialResizeCursor(point, {
                  x: layout.crop.x + layout.crop.width / 2,
                  y: layout.crop.y + layout.crop.height / 2,
                }),
              );
            move(event);
          },
        }}
      />
      <AnimatePresence>
        {activeEdges && imageRef.current ? (
          <ScreenshotCropMagnifier
            edges={activeEdges}
            inverseScale={inverseScale}
            layout={layout}
            point={magnifierPoint}
            source={source}
            sourceImage={imageRef.current}
          />
        ) : null}
      </AnimatePresence>
    </div>
  );
}
