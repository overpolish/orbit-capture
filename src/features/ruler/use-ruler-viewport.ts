// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import {
  CSSProperties,
  PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
  WheelEvent as ReactWheelEvent,
} from "react";

import { Point } from "./pixel-analysis";

const MINIMUM_ZOOM = 1;
const MAXIMUM_ZOOM = 16;
const PLAIN_WHEEL_PANS = navigator.userAgent.includes("Macintosh");

type ViewTransform = {
  panX: number;
  panY: number;
  zoom: number;
};

type WebKitGestureEvent = Event & {
  clientX: number;
  clientY: number;
  scale: number;
};

const initialTransform = (): ViewTransform => ({ panX: 0, panY: 0, zoom: 1 });

const constrained = (transform: ViewTransform): ViewTransform => {
  const maximumX = (window.innerWidth * (transform.zoom - 1)) / 2;
  const maximumY = (window.innerHeight * (transform.zoom - 1)) / 2;
  return {
    panX: Math.max(-maximumX, Math.min(maximumX, transform.panX)),
    panY: Math.max(-maximumY, Math.min(maximumY, transform.panY)),
    zoom: transform.zoom,
  };
};

export function useRulerViewport() {
  const [transform, setTransform] = useState(initialTransform);
  const panGestureRef = useRef<{
    origin: Point;
    panX: number;
    panY: number;
    pointerId: number;
  } | null>(null);

  const zoomAt = useCallback((anchor: Point, factor: number) => {
    setTransform((current) => {
      const zoom = Math.min(
        MAXIMUM_ZOOM,
        Math.max(MINIMUM_ZOOM, current.zoom * factor),
      );
      const ratio = zoom / current.zoom;
      const centeredX = anchor.x - window.innerWidth / 2;
      const centeredY = anchor.y - window.innerHeight / 2;
      return constrained({
        panX: centeredX - (centeredX - current.panX) * ratio,
        panY: centeredY - (centeredY - current.panY) * ratio,
        zoom,
      });
    });
  }, []);

  const onWheel = useCallback(
    (event: ReactWheelEvent<HTMLElement>) => {
      event.preventDefault();
      if (PLAIN_WHEEL_PANS && !event.ctrlKey) {
        setTransform((current) =>
          constrained({
            ...current,
            panX: current.panX - event.deltaX,
            panY: current.panY - event.deltaY,
          }),
        );
        return;
      }
      zoomAt(
        { x: event.clientX, y: event.clientY },
        Math.exp(-event.deltaY * 0.01),
      );
    },
    [zoomAt],
  );

  useEffect(() => {
    let previousScale = 1;
    const start = (rawEvent: Event) => {
      rawEvent.preventDefault();
      previousScale = 1;
    };
    const change = (rawEvent: Event) => {
      rawEvent.preventDefault();
      const event = rawEvent as WebKitGestureEvent;
      const factor = event.scale / previousScale;
      previousScale = event.scale;
      zoomAt({ x: event.clientX, y: event.clientY }, factor);
    };
    window.addEventListener("gesturestart", start, { passive: false });
    window.addEventListener("gesturechange", change, { passive: false });
    return () => {
      window.removeEventListener("gesturestart", start);
      window.removeEventListener("gesturechange", change);
    };
  }, [zoomAt]);

  const beginPan = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.button !== 1) return false;
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      panGestureRef.current = {
        origin: { x: event.clientX, y: event.clientY },
        panX: transform.panX,
        panY: transform.panY,
        pointerId: event.pointerId,
      };
      return true;
    },
    [transform.panX, transform.panY],
  );

  const movePan = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const gesture = panGestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return false;
    setTransform((current) =>
      constrained({
        ...current,
        panX: gesture.panX + event.clientX - gesture.origin.x,
        panY: gesture.panY + event.clientY - gesture.origin.y,
      }),
    );
    return true;
  }, []);

  const endPan = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const gesture = panGestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return false;
    panGestureRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
    return true;
  }, []);

  const toWorld = useCallback(
    (point: Point): Point => ({
      x:
        window.innerWidth / 2 +
        (point.x - window.innerWidth / 2 - transform.panX) / transform.zoom,
      y:
        window.innerHeight / 2 +
        (point.y - window.innerHeight / 2 - transform.panY) / transform.zoom,
    }),
    [transform],
  );

  const toScreen = useCallback(
    (point: Point): Point => ({
      x:
        window.innerWidth / 2 +
        (point.x - window.innerWidth / 2) * transform.zoom +
        transform.panX,
      y:
        window.innerHeight / 2 +
        (point.y - window.innerHeight / 2) * transform.zoom +
        transform.panY,
    }),
    [transform],
  );

  const reset = useCallback(() => {
    setTransform(initialTransform());
  }, []);

  const style: CSSProperties = {
    transform: `translate3d(${String(transform.panX)}px, ${String(transform.panY)}px, 0) scale(${String(transform.zoom)})`,
    transformOrigin: "50% 50%",
  };

  return {
    beginPan,
    endPan,
    isPanning: () => panGestureRef.current !== null,
    movePan,
    onWheel,
    reset,
    style,
    toScreen,
    toWorld,
    zoom: transform.zoom,
  };
}
