// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen } from "@tauri-apps/api/event";
import { RefObject, useEffect, useRef } from "react";

import {
  layoutRecordingPreviewSurface,
  requestRecordingPreviewFullResolution,
  setRecordingPreviewZoom,
} from "./api";
import { RecordingOutputSettings } from "./screenshot-output";
import { CameraOverlaySettings } from "./types";
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
  radius?: number;
};

export const applyBackdropMask = (element: HTMLElement, holes: Hole[]) => {
  const bounds = element.getBoundingClientRect();
  const key = JSON.stringify({
    height: Math.round(bounds.height * 100) / 100,
    holes,
    width: Math.round(bounds.width * 100) / 100,
  });
  if (element.dataset.previewBackdropKey === key) return;
  element.dataset.previewBackdropKey = key;
  if (holes.length === 0) {
    element.style.removeProperty("clip-path");
    element.style.removeProperty("-webkit-clip-path");
    element.style.removeProperty("mask-image");
    element.style.removeProperty("mask-size");
    element.style.removeProperty("mask-position");
    element.style.removeProperty("mask-repeat");
    element.style.removeProperty("mask-composite");
    return;
  }
  const rounded = holes.some((hole) => (hole.radius ?? 0) > 0);
  if (rounded) {
    const roundedRect = (hole: Hole) => {
      const radius = Math.min(
        Math.max(0, hole.radius ?? 0),
        hole.width / 2,
        hole.height / 2,
      );
      const right = hole.x + hole.width;
      const bottom = hole.y + hole.height;
      if (radius === 0)
        return `M ${hole.x.toString()} ${hole.y.toString()} H ${right.toString()} V ${bottom.toString()} H ${hole.x.toString()} Z`;
      return [
        `M ${(hole.x + radius).toString()} ${hole.y.toString()}`,
        `H ${(right - radius).toString()}`,
        `A ${radius.toString()} ${radius.toString()} 0 0 1 ${right.toString()} ${(hole.y + radius).toString()}`,
        `V ${(bottom - radius).toString()}`,
        `A ${radius.toString()} ${radius.toString()} 0 0 1 ${(right - radius).toString()} ${bottom.toString()}`,
        `H ${(hole.x + radius).toString()}`,
        `A ${radius.toString()} ${radius.toString()} 0 0 1 ${hole.x.toString()} ${(bottom - radius).toString()}`,
        `V ${(hole.y + radius).toString()}`,
        `A ${radius.toString()} ${radius.toString()} 0 0 1 ${(hole.x + radius).toString()} ${hole.y.toString()}`,
        "Z",
      ].join(" ");
    };
    const path = [
      `M 0 0 H ${bounds.width.toString()} V ${bounds.height.toString()} H 0 Z`,
      ...holes.map(roundedRect),
    ].join(" ");
    const clipPath = `path(evenodd, '${path}')`;
    if (
      CSS.supports("clip-path", clipPath) ||
      CSS.supports("-webkit-clip-path", clipPath)
    ) {
      element.style.setProperty("clip-path", clipPath);
      element.style.setProperty("-webkit-clip-path", clipPath);
      element.style.removeProperty("mask-image");
      element.style.removeProperty("mask-size");
      element.style.removeProperty("mask-position");
      element.style.removeProperty("mask-repeat");
      element.style.removeProperty("mask-composite");
      return;
    }
  }
  element.style.removeProperty("clip-path");
  element.style.removeProperty("-webkit-clip-path");
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

export type RecordingSelectionGestureEvent = {
  deltaX: number;
  deltaY: number;
  edges: number;
  operation:
    | "cropMove"
    | "cropResize"
    | "frameRadius"
    | "frameResize"
    | "move"
    | "radius"
    | "resize";
  paneIndex: number;
  phase: "begin" | "update" | "end" | "cancel";
  scale: number;
  cameraOverlay?: CameraOverlaySettings;
  recordingOutput?: RecordingOutputSettings;
};

export function useRecordingPreviewSurface({
  bakeCamera,
  cameraCanvasRef,
  cameraOverlay,
  isEnabled,
  isPlaying = false,
  nativeEditorOwnsLayout = false,
  nativeLayoutHasPanes,
  nativeLayoutKey,
  onError,
  onSelectionChange,
  onSelectionGesture,
  onZoomChange,
  recordingOutput,
  screenCanvasRef,
  selection,
  selectionTargets,
  sessionIdRef,
  startedRef,
  zoomPercent,
}: {
  bakeCamera: boolean;
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  cameraOverlay: CameraOverlaySettings;
  isEnabled: boolean;
  nativeLayoutHasPanes: boolean;
  nativeLayoutKey: string;
  onError: (message: string) => void;
  recordingOutput: RecordingOutputSettings;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
  sessionIdRef: RefObject<number>;
  startedRef: RefObject<boolean>;
  isPlaying?: boolean;
  nativeEditorOwnsLayout?: boolean;
  onSelectionChange?: (paneIndex: number | null) => void;
  onSelectionGesture?: (event: RecordingSelectionGestureEvent) => void;
  onZoomChange?: (zoomPercent: number) => void;
  selection?: {
    paneIndex: number;
    radiusPercent: number;
    rect: { height: number; width: number; x: number; y: number };
    cropMode?: boolean;
    image?: { height: number; width: number; x: number; y: number };
    layerId?: number;
  } | null;
  selectionTargets?:
    | {
        paneIndex: number;
        radiusPercent: number;
        rect: { height: number; width: number; x: number; y: number };
        cropMode?: boolean;
        image?: { height: number; width: number; x: number; y: number };
        layerId?: number;
      }[]
    | null;
  zoomPercent?: number;
}) {
  const nativeSurface = usePreviewCapabilities()?.nativeRecordingPreview;
  const compositionRef = useRef({ bakeCamera, cameraOverlay, recordingOutput });
  const selectionRef = useRef(selection);
  selectionRef.current = isPlaying ? null : selection;
  const selectionTargetsRef = useRef(selectionTargets);
  selectionTargetsRef.current = isPlaying ? null : selectionTargets;
  const onSelectionChangeRef = useRef(onSelectionChange);
  onSelectionChangeRef.current = onSelectionChange;
  const onSelectionGestureRef = useRef(onSelectionGesture);
  onSelectionGestureRef.current = onSelectionGesture;
  const selectionGestureActiveRef = useRef(false);
  const fullResolutionSessionRef = useRef<number | null>(null);
  const layoutRequestIdRef = useRef(0);
  const measureRef = useRef<() => void>(() => undefined);
  const nativeZoomEchoRef = useRef<number | undefined>(undefined);
  const zoomPercentRef = useRef(zoomPercent);
  zoomPercentRef.current = zoomPercent;
  compositionRef.current = { bakeCamera, cameraOverlay, recordingOutput };

  useEffect(() => {
    if (!isEnabled || !nativeEditorOwnsLayout) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<{ sessionId: number; zoomPercent: number }>(
      "recording-preview://transform",
      (event) => {
        if (
          !disposed &&
          event.payload.sessionId === sessionIdRef.current &&
          Number.isFinite(event.payload.zoomPercent)
        ) {
          const roundedZoom = Math.round(event.payload.zoomPercent);
          nativeZoomEchoRef.current =
            roundedZoom === zoomPercentRef.current ? undefined : roundedZoom;
          if (
            event.payload.zoomPercent > 100 &&
            fullResolutionSessionRef.current !== event.payload.sessionId
          ) {
            fullResolutionSessionRef.current = event.payload.sessionId;
            void requestRecordingPreviewFullResolution(
              event.payload.sessionId,
            ).catch((cause: unknown) => {
              if (!disposed) onError(String(cause));
            });
          }
          onZoomChange?.(roundedZoom);
        }
      },
    ).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isEnabled, nativeEditorOwnsLayout, onError, onZoomChange, sessionIdRef]);

  useEffect(() => {
    if (
      !isEnabled ||
      !nativeEditorOwnsLayout ||
      zoomPercent === undefined ||
      !startedRef.current
    )
      return;
    const nativeZoom = nativeZoomEchoRef.current;
    if (nativeZoom !== undefined) {
      if (nativeZoom === zoomPercent) nativeZoomEchoRef.current = undefined;
      return;
    }
    void setRecordingPreviewZoom(sessionIdRef.current, zoomPercent).catch(
      (cause: unknown) => {
        onError(String(cause));
      },
    );
  }, [
    isEnabled,
    nativeEditorOwnsLayout,
    onError,
    sessionIdRef,
    startedRef,
    zoomPercent,
  ]);

  useEffect(() => {
    if (!isEnabled || !nativeEditorOwnsLayout) return;
    let disposed = false;
    const disposers: (() => void)[] = [];
    void listen<{ paneIndex: number | null; sessionId: number }>(
      "recording-preview://selection-change",
      (event) => {
        const payload = event.payload;
        if (
          disposed ||
          payload.sessionId !== sessionIdRef.current ||
          (payload.paneIndex !== null && !Number.isInteger(payload.paneIndex))
        )
          return;
        onSelectionChangeRef.current?.(payload.paneIndex);
      },
    ).then((dispose) => {
      if (disposed) dispose();
      else disposers.push(dispose);
    });
    void listen<
      Omit<RecordingSelectionGestureEvent, "operation"> & {
        operation: number;
        sessionId: number;
      }
    >("recording-preview://selection-gesture", (event) => {
      const payload = event.payload;
      if (
        disposed ||
        payload.sessionId !== sessionIdRef.current ||
        !Number.isFinite(payload.deltaX) ||
        !Number.isFinite(payload.deltaY) ||
        !Number.isInteger(payload.edges) ||
        ![0, 1, 2, 3, 4, 5, 6].includes(payload.operation) ||
        !Number.isInteger(payload.paneIndex) ||
        !Number.isFinite(payload.scale) ||
        !["begin", "update", "end", "cancel"].includes(payload.phase)
      )
        return;
      // Crop is a React-mirrored display mode. Its uncropped composition must
      // be allowed to follow a layer selection immediately; freezing layout
      // here leaves the native OSC on the new layer over the previous layer's
      // pixels until mouse-up.
      if (payload.phase === "begin")
        selectionGestureActiveRef.current = payload.operation < 5;
      else if (payload.phase === "end" || payload.phase === "cancel")
        selectionGestureActiveRef.current = false;
      onSelectionGestureRef.current?.({
        cameraOverlay: payload.cameraOverlay,
        deltaX: payload.deltaX,
        deltaY: payload.deltaY,
        edges: payload.edges,
        operation:
          payload.operation === 0
            ? "move"
            : payload.operation === 1
              ? "resize"
              : payload.operation === 2
                ? "radius"
                : payload.operation === 3
                  ? "frameResize"
                  : payload.operation === 4
                    ? "frameRadius"
                    : payload.operation === 5
                      ? "cropMove"
                      : "cropResize",
        paneIndex: payload.paneIndex,
        phase: payload.phase,
        recordingOutput: payload.recordingOutput,
        scale: payload.scale,
      });
      if (payload.phase === "end" || payload.phase === "cancel") {
        // Pointer-rate composition is native, so React-driven layout sync is
        // suppressed during the gesture. Reconcile once React has committed
        // its final semantic frame; otherwise the native state can remain
        // newer than React until some unrelated workarea interaction.
        requestAnimationFrame(() => {
          if (!disposed) measureRef.current();
        });
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else disposers.push(dispose);
    });
    return () => {
      disposed = true;
      for (const dispose of disposers) dispose();
    };
  }, [isEnabled, nativeEditorOwnsLayout, sessionIdRef]);

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
      const requestId = ++layoutRequestIdRef.current;
      pendingLayout = {
        acknowledgeTransform:
          acknowledgeTransform ||
          (pendingLayout?.acknowledgeTransform ?? false),
        value: {
          ...value,
          requestId,
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
      // Pointer-rate recording edits are rendered directly by the retained
      // native workspace. ResizeObserver still sees React's live semantic
      // mirror changing the invisible geometry markers; feeding those bounds
      // back into native during the same gesture makes the two owners fight
      // and visibly flicker. Reconcile exactly once from the final React state
      // after end/cancel (scheduled by the gesture listener above).
      if (selectionGestureActiveRef.current) return;
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
              ...compositionRef.current,
              nativeEditor: nativeEditorOwnsLayout,
              panes: [],
              requestId: 0,
              scale: window.devicePixelRatio || 1,
              selection: null,
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
            // Punch the whole viewport, not the pane rects. The native
            // container behind the panes paints the same composited backdrop,
            // so the result looks identical - but a per-pane hole would have
            // to move in lockstep with the panes, and the webview commits its
            // layer tree from another process on its own schedule. During a
            // canvas resize that hole would land a display tick before or
            // after the native pane and shimmer along its edge.
            const holes: Hole[] =
              viewportRect.width >= 1 && viewportRect.height >= 1
                ? [
                    {
                      height: Math.round(viewportRect.height * 100) / 100,
                      width: Math.round(viewportRect.width * 100) / 100,
                      x: 0,
                      y: 0,
                    },
                  ]
                : [];
            for (const element of document.querySelectorAll<HTMLElement>(
              "[data-preview-backdrop]",
            )) {
              const elementRect = element.getBoundingClientRect();
              applyBackdropMask(
                element,
                holes.map((hole) => ({
                  ...hole,
                  x:
                    Math.round((viewportRect.left - elementRect.left) * 100) /
                    100,
                  y:
                    Math.round((viewportRect.top - elementRect.top) * 100) /
                    100,
                })),
              );
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
              ...compositionRef.current,
              nativeEditor: nativeEditorOwnsLayout,
              panes,
              requestId: 0,
              scale,
              selection: selectionRef.current,
              selectionTargets: selectionTargetsRef.current,
              sessionId: sessionIdRef.current,
              viewport: viewportSurface,
            },
            acknowledgeTransform,
          );
        }
      }
    };
    measureRef.current = () => {
      measure();
    };
    if (nativeEditorOwnsLayout && nativeSurface) {
      if (!nativeLayoutHasPanes) {
        measure();
        return () => {
          disposed = true;
          clearBackdropMasks();
          measureRef.current = () => undefined;
        };
      }
      const observer = new ResizeObserver(() => {
        measure();
      });
      let mutationObserver: MutationObserver | undefined;
      const observeMarkers = () => {
        if (disposed) return;
        const canvases = [
          screenCanvasRef.current,
          cameraCanvasRef.current,
        ].filter(
          (canvas): canvas is HTMLCanvasElement => canvas?.isConnected === true,
        );
        if (!startedRef.current || canvases.length === 0) {
          animation = requestAnimationFrame(observeMarkers);
          return;
        }
        for (const canvas of canvases) observer.observe(canvas);
        const viewport = canvases[0]?.closest<HTMLElement>(
          "[data-recording-preview-viewport]",
        );
        if (viewport) {
          observer.observe(viewport);
          mutationObserver = new MutationObserver((records) => {
            // measure() writes the backdrop mask's style inside this same
            // subtree; reacting to that write would loop forever.
            const relevant = records.some(
              (record) =>
                !(record.target instanceof HTMLElement) ||
                !record.target.closest("[data-preview-backdrop]"),
            );
            if (!relevant) return;
            for (const canvas of [
              screenCanvasRef.current,
              cameraCanvasRef.current,
            ]) {
              if (canvas?.isConnected) observer.observe(canvas);
            }
            measure();
          });
          // `attributes` matters as much as `childList`: when the workspace is
          // width-constrained, a viewport height change (the timeline loading
          // in) moves the centred marker without resizing it. ResizeObserver
          // only reports size changes, and the viewport-resize measure runs
          // before React has repositioned the marker - so the corrected
          // position only ever reaches native if the marker's style write
          // itself re-triggers a measure.
          mutationObserver.observe(viewport, {
            attributeFilter: ["style"],
            attributes: true,
            childList: true,
            subtree: true,
          });
        }
        measure();
      };
      animation = requestAnimationFrame(observeMarkers);
      return () => {
        disposed = true;
        cancelAnimationFrame(animation);
        mutationObserver?.disconnect();
        observer.disconnect();
        clearBackdropMasks();
        measureRef.current = () => undefined;
      };
    }
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
      measureRef.current = () => undefined;
    };
  }, [
    cameraCanvasRef,
    isEnabled,
    nativeEditorOwnsLayout,
    nativeLayoutHasPanes,
    nativeLayoutKey,
    nativeSurface,
    onError,
    screenCanvasRef,
    sessionIdRef,
    startedRef,
  ]);

  useEffect(() => {
    if (!isEnabled) return;
    let animation = 0;
    const updateAppearance = () => {
      cancelAnimationFrame(animation);
      animation = requestAnimationFrame(() => {
        measureRef.current();
      });
    };
    window.addEventListener("screenwide-theme-changed", updateAppearance);
    return () => {
      cancelAnimationFrame(animation);
      window.removeEventListener("screenwide-theme-changed", updateAppearance);
    };
  }, [isEnabled]);

  useEffect(() => {
    if (!selectionGestureActiveRef.current) measureRef.current();
  }, [
    bakeCamera,
    cameraOverlay,
    isPlaying,
    recordingOutput,
    selection,
    selectionTargets,
  ]);
}
