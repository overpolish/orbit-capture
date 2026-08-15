// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useEffect } from "react";

import { layoutRecordingPreviewSurface } from "./api";
import { usePreviewCapabilities } from "./use-preview-capabilities";

/**
 * The native video panes render BELOW the webview. Every element that paints
 * a background over the preview area declares `data-preview-backdrop`, and
 * this mask punches holes over the pane rects so the video shows through
 * while all DOM controls stay naturally on top. The holes are plain gradient
 * layers: an image resource (e.g. an SVG data URI) would be re-fetched every
 * time a hole changes size mid-pan, and the async load flashes the whole
 * background out. Rounded output corners are covered by the container's
 * colour-matched backstop behind the panes.
 */
export type Hole = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export const applyBackdropMask = (element: HTMLElement, holes: Hole[]) => {
  const key = JSON.stringify(holes);
  if (element.dataset.previewBackdropKey === key) return;
  element.dataset.previewBackdropKey = key;
  if (holes.length === 0) {
    element.style.removeProperty("mask-image");
    element.style.removeProperty("mask-size");
    element.style.removeProperty("mask-position");
    element.style.removeProperty("mask-repeat");
    element.style.removeProperty("mask-composite");
    return;
  }
  element.style.maskImage = [
    ...holes.map(() => "linear-gradient(#fff,#fff)"),
    "linear-gradient(#fff,#fff)",
  ].join(", ");
  element.style.maskSize = [
    ...holes.map(
      (hole) => `${hole.width.toString()}px ${hole.height.toString()}px`,
    ),
    "100% 100%",
  ].join(", ");
  element.style.maskPosition = [
    ...holes.map((hole) => `${hole.x.toString()}px ${hole.y.toString()}px`),
    "0 0",
  ].join(", ");
  element.style.maskRepeat = "no-repeat";
  element.style.maskComposite = [...holes.map(() => "exclude"), "add"].join(
    ", ",
  );
};

export type PreviewBackdrop = [number, number, number, number];

let backdropProbe: CanvasRenderingContext2D | null = null;
const backdropCache = new Map<string, PreviewBackdrop>();

const compositeBackdrop = (selector: string): PreviewBackdrop => {
  const layers = Array.from(
    document.querySelectorAll<HTMLElement>(selector),
    (element) => getComputedStyle(element).backgroundColor,
  );
  const key = `${selector}|${layers.join("|")}`;
  const cached = backdropCache.get(key);
  if (cached) return cached;
  if (!backdropProbe) {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    backdropProbe = canvas.getContext("2d", { willReadFrequently: true });
    if (!backdropProbe) return [0, 0, 0, 1];
  }
  backdropProbe.clearRect(0, 0, 1, 1);
  backdropProbe.globalCompositeOperation = "source-over";
  for (const layer of layers) {
    backdropProbe.fillStyle = layer;
    backdropProbe.fillRect(0, 0, 1, 1);
  }
  const pixel = backdropProbe.getImageData(0, 0, 1, 1).data;
  const colour: PreviewBackdrop = [
    pixel[0] / 255,
    pixel[1] / 255,
    pixel[2] / 255,
    pixel[3] / 255,
  ];
  backdropCache.set(key, colour);
  return colour;
};

/**
 * The effective viewport backdrop: the translucent background layers
 * composited bottom-up over transparency. Windows gives this RGBA surface to
 * DirectComposition, so it is blended over the same live window material as
 * the neighbouring WebView pixels instead of approximating them over black.
 * A 1x1 canvas
 * does the compositing because computed backgrounds arrive in any CSS colour
 * syntax (`rgb(... / 0.92)`, `color(srgb ...)`), all of which `fillStyle`
 * understands.
 */
export const effectiveBackdrop = (): PreviewBackdrop => {
  return compositeBackdrop("[data-preview-backdrop]");
};

export const clearBackdropMasks = () => {
  for (const element of document.querySelectorAll<HTMLElement>(
    "[data-preview-backdrop]",
  )) {
    applyBackdropMask(element, []);
  }
};

export function useRecordingPreviewSurface({
  cameraCanvasRef,
  isEnabled,
  onError,
  screenCanvasRef,
  sessionIdRef,
  startedRef,
}: {
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  isEnabled: boolean;
  onError: (message: string) => void;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  sessionIdRef: RefObject<number>;
  startedRef: RefObject<boolean>;
}) {
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  useEffect(() => {
    // Wait for the capability probe rather than guessing: masking backdrops
    // for panes that will never render would punch holes through the UI.
    if (!isEnabled || nativeSurface === undefined) return;
    let animation = 0;
    let disposed = false;
    let inFlight = false;
    let lastLayout = "";
    let pendingLayout: {
      acknowledgeTransform: boolean;
      value: Parameters<typeof layoutRecordingPreviewSurface>[0];
    } | null = null;
    let requestId = 0;
    const queueLayout = (
      value: Parameters<typeof layoutRecordingPreviewSurface>[0],
      acknowledgeTransform = false,
    ) => {
      const nextLayout = JSON.stringify(value);
      if (nextLayout === lastLayout) {
        if (acknowledgeTransform) {
          queueMicrotask(() => {
            if (!disposed) {
              window.dispatchEvent(
                new Event("screenwide-preview-transform-committed"),
              );
            }
          });
        }
        return;
      }
      lastLayout = nextLayout;
      pendingLayout = {
        acknowledgeTransform:
          acknowledgeTransform ||
          (pendingLayout?.acknowledgeTransform ?? false),
        value: {
          ...value,
          requestId: ++requestId,
        },
      };
      flush();
    };
    const flush = () => {
      if (disposed || inFlight || !pendingLayout) return;
      const next = pendingLayout;
      pendingLayout = null;
      inFlight = true;
      void layoutRecordingPreviewSurface(next.value)
        .catch((cause: unknown) => {
          if (!disposed) onError(String(cause));
        })
        .finally(() => {
          if (!disposed && next.acknowledgeTransform) {
            window.dispatchEvent(
              new Event("screenwide-preview-transform-committed"),
            );
          }
          inFlight = false;
          flush();
        });
    };
    const measure = (acknowledgeTransform = false) => {
      if (startedRef.current) {
        const connected = [screenCanvasRef.current, cameraCanvasRef.current]
          .map((canvas, index) => ({ canvas, index }))
          .filter(
            ({ canvas }) =>
              canvas?.isConnected && canvas.getBoundingClientRect().width > 0,
          );
        if (connected.length === 0) {
          clearBackdropMasks();
          queueLayout(
            {
              backdrop: effectiveBackdrop(),
              panes: [],
              requestId: 0,
              scale: window.devicePixelRatio || 1,
              sessionId: sessionIdRef.current,
              viewport: { height: 0, width: 0, x: 0, y: 0 },
            },
            acknowledgeTransform,
          );
          return;
        }
        const viewport = connected[0]?.canvas?.closest<HTMLElement>(
          "[data-recording-preview-viewport]",
        );
        if (viewport) {
          const viewportRect = viewport.getBoundingClientRect();
          const panes = connected.map(({ canvas, index }) => {
            const rect = canvas?.getBoundingClientRect() ?? new DOMRect();
            return {
              index,
              rect: {
                height: rect.height,
                width: rect.width,
                x: rect.left - viewportRect.left,
                y: rect.top - viewportRect.top,
              },
            };
          });
          if (nativeSurface) {
            for (const element of document.querySelectorAll<HTMLElement>(
              "[data-preview-backdrop]",
            )) {
              const elementRect = element.getBoundingClientRect();
              const holes: Hole[] = [];
              for (const { rect } of panes) {
                // The native panes are clipped to the viewport, so a zoomed
                // pane must not punch beyond it - that would see through UI
                // the video never covers.
                const left = Math.max(rect.x, 0);
                const top = Math.max(rect.y, 0);
                const right = Math.min(rect.x + rect.width, viewportRect.width);
                const bottom = Math.min(
                  rect.y + rect.height,
                  viewportRect.height,
                );
                if (right - left < 1 || bottom - top < 1) continue;
                holes.push({
                  height: Math.round((bottom - top) * 100) / 100,
                  width: Math.round((right - left) * 100) / 100,
                  x:
                    Math.round(
                      (viewportRect.left + left - elementRect.left) * 100,
                    ) / 100,
                  y:
                    Math.round(
                      (viewportRect.top + top - elementRect.top) * 100,
                    ) / 100,
                });
              }
              applyBackdropMask(element, holes);
            }
          }
          const viewportSurface = {
            height: viewportRect.height,
            width: viewportRect.width,
            x: viewportRect.left,
            y: viewportRect.top,
          };
          const scale = window.devicePixelRatio || 1;
          // One native layout may be in flight at a time. Intermediate DOM
          // positions are replaced by the newest one, and the Rust side also
          // rejects an older request if IPC completion order ever differs.
          queueLayout(
            {
              backdrop: effectiveBackdrop(),
              panes,
              requestId: 0,
              scale,
              sessionId: sessionIdRef.current,
              viewport: viewportSurface,
            },
            acknowledgeTransform,
          );
        }
      }
    };
    const update = () => {
      if (disposed) return;
      measure();
      animation = requestAnimationFrame(update);
    };
    // Pan/zoom transforms dispatch this right after their style write; the
    // synchronous measure keeps the native pane glued to the webview instead
    // of trailing by an animation frame of callback ordering.
    const onTransformed = (event: Event) => {
      if (disposed || !startedRef.current || !nativeSurface) return;
      event.preventDefault();
      measure(true);
    };
    window.addEventListener("screenwide-preview-transformed", onTransformed);
    animation = requestAnimationFrame(update);
    return () => {
      disposed = true;
      cancelAnimationFrame(animation);
      window.removeEventListener(
        "screenwide-preview-transformed",
        onTransformed,
      );
      clearBackdropMasks();
    };
  }, [
    cameraCanvasRef,
    isEnabled,
    nativeSurface,
    onError,
    screenCanvasRef,
    sessionIdRef,
    startedRef,
  ]);
}
