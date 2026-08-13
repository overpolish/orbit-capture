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

let backdropProbe: CanvasRenderingContext2D | null = null;
let backdropKey = "";
let backdropColour: [number, number, number] = [0, 0, 0];

/**
 * The effective viewport backdrop: the translucent background layers
 * composited bottom-up over black, matching what the user sees so the native
 * container can paint the same colour behind the video panes. A 1x1 canvas
 * does the compositing because computed backgrounds arrive in any CSS colour
 * syntax (`rgb(... / 0.92)`, `color(srgb ...)`), all of which `fillStyle`
 * understands.
 */
export const effectiveBackdrop = (): [number, number, number] => {
  const layers: string[] = [];
  for (const element of document.querySelectorAll<HTMLElement>(
    "[data-preview-backdrop]",
  )) {
    layers.push(getComputedStyle(element).backgroundColor);
  }
  const key = layers.join("|");
  if (key === backdropKey) return backdropColour;
  if (!backdropProbe) {
    const canvas = document.createElement("canvas");
    canvas.width = 1;
    canvas.height = 1;
    backdropProbe = canvas.getContext("2d", { willReadFrequently: true });
    if (!backdropProbe) return backdropColour;
  }
  backdropProbe.globalCompositeOperation = "source-over";
  backdropProbe.fillStyle = "#000";
  backdropProbe.fillRect(0, 0, 1, 1);
  for (const layer of layers) {
    backdropProbe.fillStyle = layer;
    backdropProbe.fillRect(0, 0, 1, 1);
  }
  const pixel = backdropProbe.getImageData(0, 0, 1, 1).data;
  backdropKey = key;
  backdropColour = [pixel[0] / 255, pixel[1] / 255, pixel[2] / 255];
  return backdropColour;
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
    let pendingLayout:
      Parameters<typeof layoutRecordingPreviewSurface>[0] | null = null;
    let requestId = 0;
    const flush = () => {
      if (disposed || inFlight || !pendingLayout) return;
      const next = pendingLayout;
      pendingLayout = null;
      inFlight = true;
      void layoutRecordingPreviewSurface(next)
        .catch((cause: unknown) => {
          if (!disposed) onError(String(cause));
        })
        .finally(() => {
          inFlight = false;
          flush();
        });
    };
    const measure = () => {
      if (startedRef.current) {
        const connected = [screenCanvasRef.current, cameraCanvasRef.current]
          .map((canvas, index) => ({ canvas, index }))
          .filter(
            ({ canvas }) =>
              canvas?.isConnected && canvas.getBoundingClientRect().width > 0,
          );
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
          const nextLayout = JSON.stringify({ panes, scale, viewportSurface });
          if (nextLayout !== lastLayout) {
            lastLayout = nextLayout;
            // One native layout may be in flight at a time. Intermediate DOM
            // positions are replaced by the newest one, and the Rust side also
            // rejects an older request if IPC completion order ever differs.
            pendingLayout = {
              backdrop: effectiveBackdrop(),
              panes,
              requestId: ++requestId,
              scale,
              sessionId: sessionIdRef.current,
              viewport: viewportSurface,
            };
            flush();
          }
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
    const onTransformed = () => {
      if (!disposed) measure();
    };
    window.addEventListener("orbit-preview-transformed", onTransformed);
    animation = requestAnimationFrame(update);
    return () => {
      disposed = true;
      cancelAnimationFrame(animation);
      window.removeEventListener("orbit-preview-transformed", onTransformed);
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
