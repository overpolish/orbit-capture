// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { AnimatePresence } from "motion/react";
import {
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
  RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import { flushSync } from "react-dom";

import { CropShade } from "../../../components/shared/canvas-tools/crop-shade";
import { TransformControls } from "../../../components/shared/canvas-tools/transform-controls";
import { clamp } from "../camera-overlay-geometry";
import {
  ScreenshotOutputSettings,
  screenshotLayout,
} from "../screenshot-output";
import {
  ScreenshotSnapGuide,
  snapScreenshotFrame,
} from "../screenshot-snapping";
import { useExportEditGesture } from "../use-export-edit-history";

import { ScreenshotCropMagnifier } from "./screenshot-crop-magnifier";
import {
  constrainedHandlePoint,
  cropEdgesInsideImage,
} from "./screenshot-layout-geometry";

import type { ScreenshotLayoutEdge as Edge } from "./screenshot-layout-geometry";

type Action =
  | { kind: "crop"; offsetX: number; offsetY: number }
  | { kind: "whole"; offsetX: number; offsetY: number }
  | { edges: Edge[]; kind: "resize" };

export type ScreenshotLayoutChange = {
  autoFitCanvas: boolean;
  autoFitStarted: boolean;
  settings: ScreenshotOutputSettings;
};

export function ScreenshotLayoutControl({
  controlsVisible = true,
  mediaRef,
  mode,
  onChange,
  onChangeEnd,
  onInteractionEnd,
  onInteractionStart,
  onItemContextMenu,
  output,
  previewSourceRef,
  previewUrl,
  settings,
  snapFrames = [],
  source,
}: {
  mediaRef: RefObject<HTMLDivElement | null>;
  mode: "crop" | "transform";
  output: { height: number; width: number };
  settings: ScreenshotOutputSettings;
  source: { height: number; width: number };
  controlsVisible?: boolean;
  onChange?: (
    change: ScreenshotLayoutChange,
  ) => ScreenshotOutputSettings | undefined;
  onChangeEnd?: (settings: ScreenshotOutputSettings) => void;
  onInteractionEnd?: () => void;
  onInteractionStart?: () => void;
  onItemContextMenu?: (event: ReactMouseEvent<HTMLDivElement>) => void;
  previewSourceRef?: RefObject<HTMLCanvasElement | null>;
  previewUrl?: string | null;
  snapFrames?: { height: number; width: number; x: number; y: number }[];
}) {
  const editGesture = useExportEditGesture();
  const activeRef = useRef<{
    action: Action;
    autoFitCanvas: boolean;
    clientX: number;
    clientY: number;
    output: { height: number; width: number };
    scaleX: number;
    scaleY: number;
    settings: ScreenshotOutputSettings;
  } | null>(null);
  const altPressedRef = useRef(false);
  const [activeEdges, setActiveEdges] = useState<Edge[] | null>(null);
  const [magnifierPoint, setMagnifierPoint] = useState({ x: 0, y: 0 });
  const [snapGuides, setSnapGuides] = useState<{
    x?: ScreenshotSnapGuide;
    y?: ScreenshotSnapGuide;
  }>({});
  const [draft, setDraft] = useState(settings);
  const draftRef = useRef(settings);
  const imageRef = useRef<HTMLImageElement | null>(null);
  useEffect(() => {
    const updateAlt = (event: KeyboardEvent) => {
      if (event.key === "Alt") altPressedRef.current = event.type === "keydown";
    };
    const releaseAlt = () => {
      altPressedRef.current = false;
    };
    window.addEventListener("keydown", updateAlt);
    window.addEventListener("keyup", updateAlt);
    window.addEventListener("blur", releaseAlt);
    return () => {
      window.removeEventListener("keydown", updateAlt);
      window.removeEventListener("keyup", updateAlt);
      window.removeEventListener("blur", releaseAlt);
    };
  }, []);
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
  const magnifierSource = previewSourceRef?.current ?? imageRef.current;
  const inverseScale = "var(--preview-inverse-scale, 1)";

  const naturalPoint = (event: ReactPointerEvent) => {
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return null;
    return {
      x: ((event.clientX - bounds.left) * output.width) / bounds.width,
      y: ((event.clientY - bounds.top) * output.height) / bounds.height,
    };
  };
  const begin = (event: ReactPointerEvent, action: Action) => {
    // Middle mouse belongs to the parent preview viewport everywhere,
    // including over crop fills, handles and the scale ring.
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const bounds = mediaRef.current?.getBoundingClientRect();
    if (!bounds || bounds.width === 0 || bounds.height === 0) return;
    editGesture.beginGesture();
    onInteractionStart?.();
    altPressedRef.current = event.altKey;
    activeRef.current = {
      action,
      autoFitCanvas: altPressedRef.current,
      clientX: event.clientX,
      clientY: event.clientY,
      output,
      scaleX: output.width / bounds.width,
      scaleY: output.height / bounds.height,
      settings: draftRef.current,
    };
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
    let active = activeRef.current;
    const point = naturalPoint(event);
    if (!active || !point) return;
    event.preventDefault();
    event.stopPropagation();
    const autoFitCanvas = altPressedRef.current;
    let autoFitStarted = false;
    if (
      active.action.kind === "whole" &&
      active.autoFitCanvas !== autoFitCanvas
    ) {
      const bounds = mediaRef.current?.getBoundingClientRect();
      if (!bounds || bounds.width === 0 || bounds.height === 0) return;
      autoFitStarted = autoFitCanvas;
      active = {
        ...active,
        autoFitCanvas,
        clientX: event.clientX,
        clientY: event.clientY,
        output,
        scaleX: output.width / bounds.width,
        scaleY: output.height / bounds.height,
        settings: draftRef.current,
      };
      activeRef.current = active;
    }
    const gestureOutput = active.output;
    const start = screenshotLayout(source, gestureOutput, active.settings);
    const next = { ...active.settings };
    if (active.action.kind === "crop" || active.action.kind === "whole") {
      let rawX =
        active.action.kind === "whole"
          ? start.crop.x + (event.clientX - active.clientX) * active.scaleX
          : point.x - active.action.offsetX;
      let rawY =
        active.action.kind === "whole"
          ? start.crop.y + (event.clientY - active.clientY) * active.scaleY
          : point.y - active.action.offsetY;
      if (active.action.kind === "whole" && event.shiftKey) {
        const deltaX = rawX - start.crop.x;
        const deltaY = rawY - start.crop.y;
        if (Math.abs(deltaX) >= Math.abs(deltaY)) rawY = start.crop.y;
        else rawX = start.crop.x;
      }
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
        const snapped = snapScreenshotFrame({
          canvas: gestureOutput,
          frame: start.crop,
          objects: snapFrames,
          position: { x, y },
          thresholdX: active.scaleX * 8,
          thresholdY: active.scaleY * 8,
        });
        x = snapped.position.x;
        y = snapped.position.y;
        setSnapGuides(snapped.guides);
      } else {
        setSnapGuides({});
      }
      const deltaX = x - start.crop.x;
      const deltaY = y - start.crop.y;
      next.screenshotCropXPercent = (x * 100) / gestureOutput.width;
      next.screenshotCropYPercent = (y * 100) / gestureOutput.height;
      if (active.action.kind === "whole") {
        next.screenshotImageXPercent += (deltaX * 100) / gestureOutput.width;
        next.screenshotImageYPercent += (deltaY * 100) / gestureOutput.height;
      }
    } else if (mode === "crop") {
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
      const { edges } = active.action;
      const centered = event.altKey;
      const anchorX = centered
        ? start.crop.x + start.crop.width / 2
        : edges.includes("left")
          ? start.crop.x + start.crop.width
          : edges.includes("right")
            ? start.crop.x
            : start.crop.x + start.crop.width / 2;
      const anchorY = centered
        ? start.crop.y + start.crop.height / 2
        : edges.includes("top")
          ? start.crop.y + start.crop.height
          : edges.includes("bottom")
            ? start.crop.y
            : start.crop.y + start.crop.height / 2;
      const handleX = edges.includes("left")
        ? start.crop.x
        : edges.includes("right")
          ? start.crop.x + start.crop.width
          : start.crop.x + start.crop.width / 2;
      const handleY = edges.includes("top")
        ? start.crop.y
        : edges.includes("bottom")
          ? start.crop.y + start.crop.height
          : start.crop.y + start.crop.height / 2;
      const vectorX = handleX - anchorX;
      const vectorY = handleY - anchorY;
      const denominator = vectorX * vectorX + vectorY * vectorY;
      const minimum = Math.max(
        2,
        (36 * output.width) / (mediaRef.current?.clientWidth || 1),
      );
      const scale = clamp(
        denominator > 0
          ? ((point.x - anchorX) * vectorX + (point.y - anchorY) * vectorY) /
              denominator
          : 1,
        minimum / Math.max(1, start.crop.width),
        8,
      );
      const transform = (value: number, anchor: number) =>
        anchor + (value - anchor) * scale;
      const cropX = transform(start.crop.x, anchorX);
      const cropY = transform(start.crop.y, anchorY);
      const imageX = transform(start.image.x, anchorX);
      const imageY = transform(start.image.y, anchorY);
      next.screenshotCropWidthPercent =
        (start.crop.width * scale * 100) / output.width;
      next.screenshotCropHeightPercent =
        (start.crop.height * scale * 100) / output.height;
      next.screenshotCropXPercent = (cropX * 100) / output.width;
      next.screenshotCropYPercent = (cropY * 100) / output.height;
      next.screenshotImageWidthPercent =
        (start.image.width * scale * 100) / output.width;
      next.screenshotImageXPercent =
        ((imageX + (start.image.width * scale) / 2) * 100) / output.width;
      next.screenshotImageYPercent =
        ((imageY + (start.image.height * scale) / 2) * 100) / output.height;
    }
    const committed =
      onChange?.({
        autoFitCanvas: active.action.kind === "whole" && autoFitCanvas,
        autoFitStarted,
        settings: next,
      }) ?? next;
    draftRef.current = committed;
    // eslint-disable-next-line @eslint-react/dom-no-flush-sync
    flushSync(() => {
      setDraft(committed);
    });
  };
  const finish = (event: ReactPointerEvent) => {
    event.stopPropagation();
    onChangeEnd?.(draftRef.current);
    activeRef.current = null;
    onInteractionEnd?.();
    editGesture.endGesture();
    setActiveEdges(null);
    setSnapGuides({});
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
    <>
      {controlsVisible && mode === "crop" ? (
        <CropShade
          crop={layout.crop}
          image={layout.image}
          radius={
            (Math.min(layout.crop.width, layout.crop.height) *
              draft.radiusPercent) /
            100
          }
        />
      ) : null}
      {snapGuides.x ? (
        <div
          className={`pointer-events-none absolute top-0 h-full ${snapGuides.x.source === "object" ? "bg-info" : "bg-warning"}`}
          style={{
            left: snapGuides.x.value,
            width: `calc(1px * ${inverseScale})`,
          }}
        />
      ) : null}
      {snapGuides.y ? (
        <div
          className={`pointer-events-none absolute left-0 w-full ${snapGuides.y.source === "object" ? "bg-info" : "bg-warning"}`}
          style={{
            height: `calc(1px * ${inverseScale})`,
            top: snapGuides.y.value,
          }}
        />
      ) : null}
      <div
        className="pointer-events-none absolute touch-none"
        onContextMenu={onItemContextMenu}
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
                kind: mode === "crop" ? "crop" : "whole",
                offsetX: point.x - layout.crop.x,
                offsetY: point.y - layout.crop.y,
              });
          }}
          {...interaction}
        />
        {controlsVisible ? (
          <TransformControls
            frame={{
              height: layout.crop.height,
              width: layout.crop.width,
              x: 0,
              y: 0,
            }}
            interaction={interaction}
            inverseScale={inverseScale}
            lineStyle={mode === "crop" ? "dashed" : "solid"}
            resize={{
              label: (edges) =>
                `${mode === "crop" ? "Crop" : "Resize"} screenshot ${edges.join(" ")}`,
              onPointerDown: (edges) => (event) => {
                begin(event, { edges, kind: "resize" });
              },
            }}
          />
        ) : null}
        <AnimatePresence>
          {controlsVisible &&
          mode === "crop" &&
          activeEdges &&
          magnifierSource ? (
            <ScreenshotCropMagnifier
              edges={activeEdges}
              inverseScale={inverseScale}
              layout={layout}
              point={magnifierPoint}
              source={source}
              sourceImage={magnifierSource}
            />
          ) : null}
        </AnimatePresence>
      </div>
    </>
  );
}
