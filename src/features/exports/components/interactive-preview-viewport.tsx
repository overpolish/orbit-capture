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
  containTransform,
  FIT,
  maximumZoom,
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
};

/** Shared transform surface for still-image and native canvas previews. */
export function InteractivePreviewViewport<Element extends HTMLElement>({
  getMediaSize,
  hideUntilMeasured = false,
  onNeedFullResolution,
  renderMedia,
  resetKey,
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
  const requestedFullRef = useRef(false);
  const [zoomPercent, setZoomPercent] = useState(100);

  getMediaSizeRef.current = getMediaSize;
  onNeedFullResolutionRef.current = onNeedFullResolution;

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
    const inverse = scale > 0 ? 1 / scale : 1;
    media.style.boxShadow = `0 ${(2 * inverse).toString()}px ${(12 * inverse).toString()}px rgb(0 0 0 / 0.28)`;
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
    transformRef.current = containTransform(
      transformRef.current,
      geometryRef.current,
    );
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
      setZoomPercent(Math.round(transformRef.current.zoom * 100));
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
      // A native window can become visible after its first layout effects.
      // Measuring on the gesture makes that lifecycle irrelevant and ensures
      // the very first pinch uses the displayed media's real fit geometry.
      measureAndApply();
      clearTransition();
      const current = transformRef.current;
      const isPinch = isMac && event.ctrlKey;
      if (!isMac || event.ctrlKey) {
        const rate = isPinch ? PINCH_ZOOM_RATE : WHEEL_ZOOM_RATE;
        const zoom = clamp(
          current.zoom * Math.exp(-event.deltaY * rate),
          FIT,
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
        transformRef.current = containTransform(
          {
            x: pointerX - (pointerX - current.x) * ratio,
            y: pointerY - (pointerY - current.y) * ratio,
            zoom,
          },
          geometryRef.current,
        );
      } else {
        if (current.zoom <= FIT) return;
        transformRef.current = containTransform(
          {
            x: current.x - event.deltaX,
            y: current.y - event.deltaY,
            zoom: current.zoom,
          },
          geometryRef.current,
        );
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
      className="relative flex h-[280px] shrink-0 touch-none items-center justify-center overflow-hidden overscroll-contain rounded-md"
      onDoubleClick={() => {
        measureAndApply();
        reset();
      }}
      onPointerDown={(event) => {
        measureAndApply();
        if (transformRef.current.zoom <= FIT) return;
        clearTransition();
        event.currentTarget.setPointerCapture(event.pointerId);
        panRef.current = {
          pointerX: event.clientX,
          pointerY: event.clientY,
          start: transformRef.current,
        };
      }}
      onPointerMove={(event) => {
        const pan = panRef.current;
        if (!pan) return;
        transformRef.current = containTransform(
          {
            x: pan.start.x + event.clientX - pan.pointerX,
            y: pan.start.y + event.clientY - pan.pointerY,
            zoom: pan.start.zoom,
          },
          geometryRef.current,
        );
        schedule();
      }}
      onPointerUp={(event) => {
        panRef.current = null;
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      ref={boxRef}
      style={{ cursor: zoomPercent > 100 ? "grab" : "default" }}
    >
      {renderMedia({
        onReady: measureAndApply,
        ref: (element) => {
          mediaRef.current = element;
          if (element) measureAndApply();
        },
        style: { transformOrigin: "center center" },
      })}
      {zoomPercent > 100 ? (
        <span className="pointer-events-none absolute right-2 bottom-2 z-10 rounded bg-content/80 px-1.5 py-0.5 text-xxs text-muted tabular-nums">
          {zoomPercent}% double-click to fit
        </span>
      ) : null}
    </div>
  );
}
