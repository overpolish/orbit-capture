// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useCallback, useEffect, useRef, useState } from "react";

import { drawRecordingPreviewFrame } from "./recording-preview-frame";
import { RecordingPreviewLayout } from "./types";

export function useRecordingPreviewFrames({
  cameraCanvasRef,
  onError,
  screenCanvasRef,
}: {
  cameraCanvasRef: RefObject<HTMLCanvasElement | null>;
  onError: (message: string) => void;
  screenCanvasRef: RefObject<HTMLCanvasElement | null>;
}) {
  const layoutRef = useRef<RecordingPreviewLayout | null>(null);
  const latestFrameRef = useRef<ArrayBuffer | null>(null);
  const displayedFrameRef = useRef<ArrayBuffer | null>(null);
  const displayedTargetsRef = useRef<{
    camera: HTMLCanvasElement | null;
    screen: HTMLCanvasElement | null;
  }>({ camera: null, screen: null });
  const displayedFrameRequestRef = useRef(0);
  const [previewLayout, setPreviewLayout] =
    useState<RecordingPreviewLayout | null>(null);
  const [isPreparing, setIsPreparing] = useState(true);

  useEffect(() => {
    let decodeInFlight = false;
    let disposed = false;
    let animation = 0;
    const render = () => {
      const camera = cameraCanvasRef.current;
      const screen = screenCanvasRef.current;
      const targetsChanged =
        displayedTargetsRef.current.camera !== camera ||
        displayedTargetsRef.current.screen !== screen;
      const frame =
        latestFrameRef.current ??
        (targetsChanged ? displayedFrameRef.current : null);
      const currentLayout = layoutRef.current;
      if (!decodeInFlight && frame && currentLayout) {
        if (frame === latestFrameRef.current) latestFrameRef.current = null;
        decodeInFlight = true;
        void drawRecordingPreviewFrame({
          camera,
          frame,
          isCurrentRequest: (requestId) => {
            if (requestId < displayedFrameRequestRef.current) return false;
            displayedFrameRequestRef.current = requestId;
            return true;
          },
          layout: currentLayout,
          screen,
        })
          .then((drawn) => {
            if (!disposed && drawn) {
              displayedFrameRef.current = frame;
              displayedTargetsRef.current = { camera, screen };
              setIsPreparing(false);
            }
          })
          .catch((cause: unknown) => {
            if (!disposed) {
              onError(String(cause));
              setIsPreparing(false);
            }
          })
          .finally(() => {
            decodeInFlight = false;
          });
      }
      animation = requestAnimationFrame(render);
    };
    animation = requestAnimationFrame(render);
    return () => {
      disposed = true;
      cancelAnimationFrame(animation);
    };
  }, [cameraCanvasRef, onError, screenCanvasRef]);

  const begin = useCallback(() => {
    displayedFrameRequestRef.current = 0;
    setIsPreparing(true);
  }, []);
  const receive = useCallback((frame: ArrayBuffer) => {
    latestFrameRef.current = frame;
  }, []);
  const reset = useCallback(() => {
    latestFrameRef.current = null;
    displayedFrameRef.current = null;
    displayedTargetsRef.current = { camera: null, screen: null };
  }, []);
  const setLayout = useCallback((next: RecordingPreviewLayout) => {
    layoutRef.current = next;
    setPreviewLayout(next);
  }, []);

  return {
    begin,
    isPreparing,
    layout: previewLayout,
    receive,
    reset,
    setIsPreparing,
    setLayout,
  };
}
