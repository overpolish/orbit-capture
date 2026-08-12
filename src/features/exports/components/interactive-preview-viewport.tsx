// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  CSSProperties,
  ReactNode,
  RefCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import {
  clamp,
  FIT,
  maximumZoom,
  MINIMUM_ZOOM,
  PreviewGeometry,
  PreviewTransform,
} from "./preview-transform";

const FIT_SHADOW_GUTTER = 8;
const PINCH_ZOOM_RATE = 0.01;
const RESET_TRANSITION = "transform 160ms ease-out";
const WHEEL_ZOOM_RATE = 0.0015;
const isMac = navigator.userAgent.includes("Mac");

type MediaSize = { height: number; width: number };

type InteractivePreviewViewportProps<Element extends HTMLElement> = {
  getMediaSize: (element: Element) => MediaSize | null;
  renderMedia: (props: {
    onReady: () => void;
    ref: RefCallback<Element>;
    style: CSSProperties;
  }) => ReactNode;
  resetKey: string | number;
  hideUntilMeasured?: boolean;
  onNeedFullResolution?: () => void;
  onZoomChange?: (zoomPercent: number) => void;
  zoomPercent?: number;
};

/** Shared transform surface for still-image and native canvas previews. */
export function InteractivePreviewViewport<Element extends HTMLElement>({
  getMediaSize,
  hideUntilMeasured = false,
  onNeedFullResolution,
  onZoomChange,
  renderMedia,
  resetKey,
  zoomPercent: controlledZoomPercent,
}: InteractivePreviewViewportProps<Element>) {
  const boxRef = useRef<HTMLDivElement>(null);
  const mediaRef = useRef<Element | null>(null);
  const frameRef = useRef<number | undefined>(undefined);
  const panRef = useRef<{
    pointerX: number;
    pointerY: number;
    start: PreviewTransform;
  } | null>(null);
  const transformRef = useRef<PreviewTransform>({ x: 0, y: 0, zoom: FIT });
  const geometryRef = useRef<PreviewGeometry>({
    boxHeight: 0,
    boxWidth: 0,
    fitScale: 1,
    naturalHeight: 0,
    naturalWidth: 0,
  });
  const getMediaSizeRef = useRef(getMediaSize);
  const onNeedFullResolutionRef = useRef(onNeedFullResolution);
  const onZoomChangeRef = useRef(onZoomChange);
  const reportedZoomRef = useRef<number | undefined>(undefined);
  const requestedFullRef = useRef(false);
  const [isPanning, setIsPanning] = useState(false);

  getMediaSizeRef.current = getMediaSize;
  onNeedFullResolutionRef.current = onNeedFullResolution;
  onZoomChangeRef.current = onZoomChange;

  const applyTransform = (reveal: boolean) => {
    const media = mediaRef.current;
    if (!media) return;
    const { fitScale } = geometryRef.current;
    const { x, y, zoom } = transformRef.current;
    const scale = fitScale * zoom;
    media.style.transform = `translate(${x.toString()}px, ${y.toString()}px) scale(${scale.toString()})`;
    media.style.setProperty(
      "--preview-inverse-scale",
      scale > 0 ? (1 / scale).toString() : "1",
    );
    if (reveal) media.style.opacity = "1";
  };

  const measureAndApply = () => {
    const box = boxRef.current;
    const media = mediaRef.current;
    if (!box || !media) return;
    const size = getMediaSizeRef.current(media);
    if (!size || box.clientWidth === 0 || box.clientHeight === 0) return;
    const fitScale = Math.min(
      1,
      Math.max(0, box.clientWidth - FIT_SHADOW_GUTTER * 2) / size.width,
      Math.max(0, box.clientHeight - FIT_SHADOW_GUTTER * 2) / size.height,
    );
    geometryRef.current = {
      boxHeight: box.clientHeight,
      boxWidth: box.clientWidth,
      fitScale,
      naturalHeight: size.height,
      naturalWidth: size.width,
    };
    applyTransform(true);
  };

  const schedule = () => {
    // WebKit can discard a frame requested while a native window is hidden
    // without invoking its callback. Cancelling and replacing the last frame
    // prevents that stale id from permanently blocking transform UI updates.
    if (frameRef.current !== undefined) cancelAnimationFrame(frameRef.current);
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = undefined;
      applyTransform(false);
      const nextZoomPercent = Math.round(transformRef.current.zoom * 100);
      if (nextZoomPercent !== reportedZoomRef.current) {
        reportedZoomRef.current = nextZoomPercent;
        onZoomChangeRef.current?.(nextZoomPercent);
      }
    });
  };

  const clearTransition = () => {
    if (mediaRef.current) mediaRef.current.style.transition = "";
  };

  const reset = () => {
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    if (mediaRef.current) mediaRef.current.style.transition = RESET_TRANSITION;
    applyTransform(false);
    schedule();
  };

  useLayoutEffect(() => {
    requestedFullRef.current = false;
    transformRef.current = { x: 0, y: 0, zoom: FIT };
    if (mediaRef.current) {
      mediaRef.current.style.transition = "";
      if (hideUntilMeasured) mediaRef.current.style.opacity = "0";
    }
    schedule();
    measureAndApply();
    // A reset is intentionally tied to media identity, not callback identity.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [hideUntilMeasured, resetKey]);

  useEffect(() => {
    if (controlledZoomPercent === undefined) return;
    // The toolbar mirrors the live transform as a whole percentage. Do not
    // feed that rounded value back into an in-progress pinch and discard its
    // sub-percent movement.
    if (controlledZoomPercent === reportedZoomRef.current) return;
    const current = transformRef.current;
    const zoom = clamp(
      controlledZoomPercent / 100,
      MINIMUM_ZOOM,
      maximumZoom(geometryRef.current),
    );
    if (zoom === current.zoom) return;
    if (zoom > FIT && !requestedFullRef.current) {
      requestedFullRef.current = true;
      onNeedFullResolutionRef.current?.();
    }
    const ratio = zoom / current.zoom;
    transformRef.current = {
      x: current.x * ratio,
      y: current.y * ratio,
      zoom,
    };
    clearTransition();
    schedule();
    // The transform helpers operate on refs so changing zoom does not rebuild
    // the native preview surface.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, [controlledZoomPercent]);

  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const observer = new ResizeObserver(measureAndApply);
    observer.observe(box);
    return () => {
      observer.disconnect();
    };
    // The observer reads the current element and callback through refs.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, []);

  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      if (geometryRef.current.boxWidth === 0) measureAndApply();
      clearTransition();
      const current = transformRef.current;
      const isPinch = isMac && event.ctrlKey;
      if (!isMac || event.ctrlKey) {
        const rate = isPinch ? PINCH_ZOOM_RATE : WHEEL_ZOOM_RATE;
        const zoom = clamp(
          current.zoom * Math.exp(-event.deltaY * rate),
          MINIMUM_ZOOM,
          maximumZoom(geometryRef.current),
        );
        if (zoom === current.zoom) return;
        if (zoom > FIT && !requestedFullRef.current) {
          requestedFullRef.current = true;
          onNeedFullResolutionRef.current?.();
        }
        const bounds = box.getBoundingClientRect();
        const pointerX = event.clientX - (bounds.left + bounds.width / 2);
        const pointerY = event.clientY - (bounds.top + bounds.height / 2);
        const ratio = zoom / current.zoom;
        transformRef.current = {
          x: pointerX - (pointerX - current.x) * ratio,
          y: pointerY - (pointerY - current.y) * ratio,
          zoom,
        };
      } else {
        transformRef.current = {
          x: current.x - event.deltaX,
          y: current.y - event.deltaY,
          zoom: current.zoom,
        };
      }
      schedule();
    };
    box.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      box.removeEventListener("wheel", onWheel);
    };
    // The native handler reads mutable interaction state through refs.
    // eslint-disable-next-line @eslint-react/exhaustive-deps
  }, []);

  useEffect(
    () => () => {
      if (frameRef.current !== undefined)
        cancelAnimationFrame(frameRef.current);
    },
    [],
  );

  return (
    <div
      className="relative flex min-h-0 grow touch-none items-center justify-center overflow-hidden overscroll-contain bg-black/5 dark:bg-black/25"
      onDoubleClick={() => {
        measureAndApply();
        reset();
      }}
      onPointerCancel={() => {
        panRef.current = null;
        setIsPanning(false);
      }}
      onPointerDown={(event) => {
        measureAndApply();
        clearTransition();
        event.currentTarget.setPointerCapture(event.pointerId);
        setIsPanning(true);
        panRef.current = {
          pointerX: event.clientX,
          pointerY: event.clientY,
          start: transformRef.current,
        };
      }}
      onPointerMove={(event) => {
        const pan = panRef.current;
        if (!pan) return;
        transformRef.current = {
          x: pan.start.x + event.clientX - pan.pointerX,
          y: pan.start.y + event.clientY - pan.pointerY,
          zoom: pan.start.zoom,
        };
        schedule();
      }}
      onPointerUp={(event) => {
        panRef.current = null;
        setIsPanning(false);
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      ref={boxRef}
      style={{ cursor: isPanning ? "grabbing" : "grab" }}
    >
      {renderMedia({
        onReady: measureAndApply,
        ref: (element) => {
          mediaRef.current = element;
          if (element) measureAndApply();
        },
        style: { transformOrigin: "center center" },
      })}
    </div>
  );
}
