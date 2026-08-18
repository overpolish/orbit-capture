// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";

import { startCameraPreview, stopCameraPreview } from "./camera-preview-api";
import { CameraResolution } from "./types";

const FRAME_HEADER_LENGTH = 9;
const FRAME_TYPE_MJPEG = 0;

const drawFrame = async (canvas: HTMLCanvasElement, frame: ArrayBuffer) => {
  if (frame.byteLength <= FRAME_HEADER_LENGTH) return false;

  const header = new DataView(frame, 0, FRAME_HEADER_LENGTH);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const frameType = header.getUint8(8);
  if (width === 0 || height === 0) return false;

  const context = canvas.getContext("2d", { alpha: false });
  if (!context) return false;
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;

  const bitmap =
    frameType === FRAME_TYPE_MJPEG
      ? await createImageBitmap(
          new Blob([frame.slice(FRAME_HEADER_LENGTH)], { type: "image/jpeg" }),
        )
      : await createImageBitmap(
          new ImageData(
            new Uint8ClampedArray(frame, FRAME_HEADER_LENGTH),
            width,
            height,
          ),
        );
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  return true;
};

type UseCameraPreviewOptions = {
  active: boolean;
  deviceId?: string;
  mode?: CameraResolution;
};

export const useCameraPreview = ({
  active,
  deviceId,
  mode,
}: UseCameraPreviewOptions) => {
  // Keyed on the mode's primitive fields, not the object: the camera list is
  // re-fetched (and its mode objects rebuilt) every time the selector opens, so
  // identity changes constantly while the selected capture mode does not.
  const { fps, height, width } = mode ?? ({} as Partial<CameraResolution>);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const latestFrameRef = useRef<ArrayBuffer | null>(null);
  const operationsRef = useRef(Promise.resolve());
  const [hasFrame, setHasFrame] = useState(false);

  useEffect(() => {
    let decodeInFlight = false;
    let disposed = false;
    let frameRequest = 0;

    const renderLatestFrame = () => {
      const canvas = canvasRef.current;
      const frame = latestFrameRef.current;
      if (!decodeInFlight && canvas && frame) {
        latestFrameRef.current = null;
        decodeInFlight = true;
        void drawFrame(canvas, frame)
          .then((drawn) => {
            if (!disposed && drawn) setHasFrame(true);
          })
          .catch(() => undefined)
          .finally(() => {
            decodeInFlight = false;
          });
      }
      frameRequest = requestAnimationFrame(renderLatestFrame);
    };

    frameRequest = requestAnimationFrame(renderLatestFrame);
    return () => {
      disposed = true;
      cancelAnimationFrame(frameRequest);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    latestFrameRef.current = null;
    operationsRef.current = operationsRef.current
      .then(async () => {
        setHasFrame(false);
        await stopCameraPreview();
        if (!active || !deviceId || cancelled) return;
        if (fps === undefined || height === undefined || width === undefined)
          return;

        const channel = new Channel<ArrayBuffer>();
        channel.onmessage = (frame) => {
          if (!cancelled) latestFrameRef.current = frame;
        };
        await startCameraPreview(deviceId, { fps, height, width }, channel);
      })
      .catch((error: unknown) => {
        console.error("Could not start camera preview", error);
        if (!cancelled) setHasFrame(false);
      });

    return () => {
      cancelled = true;
      latestFrameRef.current = null;
      operationsRef.current = operationsRef.current
        .then(stopCameraPreview)
        .catch(() => undefined);
    };
  }, [active, deviceId, fps, height, width]);

  return { canvasRef, hasFrame };
};
