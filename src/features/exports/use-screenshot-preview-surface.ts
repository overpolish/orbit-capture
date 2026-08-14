// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useEffect, useLayoutEffect, useRef } from "react";

import {
  layoutScreenshotPreviewSurface,
  setScreenshotPreviewOutput,
  startScreenshotPreview,
  stopScreenshotPreview,
} from "./api";
import { ScreenshotOutputSettings } from "./screenshot-output";
import {
  applyBackdropMask,
  clearBackdropMasks,
  effectiveBackdrop,
  Hole,
} from "./use-recording-preview-surface";

let sessionSequence = 0;

/**
 * The native screenshot editing preview: the composed output renders on the
 * pane surface below the webview (the canvas is only a geometry marker), so
 * every settings change is a single GPU pass with no pixels crossing IPC.
 */
export function useScreenshotPreviewSurface({
  artifactId,
  canvasRef,
  isEnabled,
  output,
}: {
  artifactId: number;
  canvasRef: RefObject<HTMLElement | null>;
  isEnabled: boolean;
  output?: ScreenshotOutputSettings;
}) {
  const sessionIdRef = useRef(0);
  const startedRef = useRef(false);
  const outputRef = useRef(output);
  const presentationRef = useRef<Promise<void>>(Promise.resolve());
  outputRef.current = output;

  useEffect(() => {
    if (!isEnabled) return;
    let disposed = false;
    const sessionId = Date.now() * 1_000 + (++sessionSequence % 1_000);
    sessionIdRef.current = sessionId;
    void startScreenshotPreview(artifactId, sessionId)
      .then(() => {
        if (disposed) return;
        startedRef.current = true;
        const current = outputRef.current;
        if (current) {
          const presentation = setScreenshotPreviewOutput(
            current,
            sessionId,
          ).then(
            () => undefined,
            () => undefined,
          );
          presentationRef.current = presentation;
          void presentation;
        }
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      startedRef.current = false;
      void stopScreenshotPreview(sessionId).catch(() => undefined);
    };
  }, [artifactId, isEnabled]);

  useLayoutEffect(() => {
    if (!isEnabled || !output || !startedRef.current) return;
    const presentation = setScreenshotPreviewOutput(
      output,
      sessionIdRef.current,
    ).then(
      () => undefined,
      () => undefined,
    );
    presentationRef.current = presentation;
    void presentation;
  }, [isEnabled, output]);

  useEffect(() => {
    if (!isEnabled) return;
    let animation = 0;
    let disposed = false;
    let inFlight = false;
    let lastLayout = "";
    let pendingLayout:
      Parameters<typeof layoutScreenshotPreviewSurface>[0] | null = null;
    const isDisposed = () => disposed;
    const waitForLatestPresentation = async () => {
      while (!disposed) {
        const presentation = presentationRef.current;
        await presentation;
        if (presentation === presentationRef.current) return;
      }
    };
    const flush = () => {
      if (disposed || inFlight || !pendingLayout) return;
      inFlight = true;
      void (async () => {
        await waitForLatestPresentation();
        if (isDisposed()) return;
        const next = pendingLayout;
        pendingLayout = null;
        await layoutScreenshotPreviewSurface(next);
      })()
        .catch(() => undefined)
        .finally(() => {
          inFlight = false;
          flush();
        });
    };
    const measure = () => {
      const marker = canvasRef.current;
      if (
        startedRef.current &&
        marker?.isConnected &&
        marker.getBoundingClientRect().width > 0
      ) {
        const viewport = marker.closest<HTMLElement>(
          "[data-recording-preview-viewport]",
        );
        if (viewport) {
          const viewportRect = viewport.getBoundingClientRect();
          const rect = marker.getBoundingClientRect();
          const pane = {
            height: rect.height,
            width: rect.width,
            x: rect.left - viewportRect.left,
            y: rect.top - viewportRect.top,
          };
          for (const element of document.querySelectorAll<HTMLElement>(
            "[data-preview-backdrop]",
          )) {
            const elementRect = element.getBoundingClientRect();
            const holes: Hole[] = [];
            const left = Math.max(pane.x, 0);
            const top = Math.max(pane.y, 0);
            const right = Math.min(pane.x + pane.width, viewportRect.width);
            const bottom = Math.min(pane.y + pane.height, viewportRect.height);
            if (right - left >= 1 && bottom - top >= 1) {
              holes.push({
                height: Math.round((bottom - top) * 100) / 100,
                width: Math.round((right - left) * 100) / 100,
                x:
                  Math.round(
                    (viewportRect.left + left - elementRect.left) * 100,
                  ) / 100,
                y:
                  Math.round((viewportRect.top + top - elementRect.top) * 100) /
                  100,
              });
            }
            applyBackdropMask(element, holes);
          }
          const viewportSurface = {
            height: viewportRect.height,
            width: viewportRect.width,
            x: viewportRect.left,
            y: viewportRect.top,
          };
          const scale = window.devicePixelRatio || 1;
          const nextLayout = JSON.stringify({
            pane,
            scale,
            viewportSurface,
          });
          if (nextLayout !== lastLayout) {
            lastLayout = nextLayout;
            pendingLayout = {
              backdrop: effectiveBackdrop(),
              panes: [{ index: 0, rect: pane }],
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
  }, [canvasRef, isEnabled]);
}
