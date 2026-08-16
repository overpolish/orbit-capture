// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { RefObject, useEffect, useRef } from "react";

import { renderScreenshotOutputPreview } from "../api";
import { ScreenshotWorkspaceOutputSettings } from "../screenshot-output";

const HEADER_LENGTH = 12;

export function NativeOutputPreview({
  artifactId,
  canvasRef,
  onReady,
  output,
}: {
  artifactId: number;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  output: ScreenshotWorkspaceOutputSettings;
  onReady?: () => void;
}) {
  const requestRef = useRef(0);
  const onReadyRef = useRef(onReady);
  onReadyRef.current = onReady;

  useEffect(() => {
    let disposed = false;
    const requestId = ++requestRef.current;
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (payload) => {
      if (disposed || payload.byteLength <= HEADER_LENGTH) return;
      const header = new DataView(payload, 0, HEADER_LENGTH);
      if (header.getUint32(0, true) !== requestRef.current) return;
      const width = header.getUint32(4, true);
      const height = header.getUint32(8, true);
      const expectedLength = width * height * 4;
      if (payload.byteLength !== HEADER_LENGTH + expectedLength) return;
      const canvas = canvasRef.current;
      if (!canvas || requestId !== requestRef.current) return;
      if (canvas.width !== width) canvas.width = width;
      if (canvas.height !== height) canvas.height = height;
      const pixels = new Uint8ClampedArray(
        payload,
        HEADER_LENGTH,
        expectedLength,
      );
      canvas
        .getContext("2d", { alpha: true })
        ?.putImageData(new ImageData(pixels, width, height), 0, 0);
      onReadyRef.current?.();
    };
    void renderScreenshotOutputPreview({
      artifactId,
      channel,
      output,
      requestId,
    });
    return () => {
      disposed = true;
    };
  }, [artifactId, canvasRef, output]);

  return (
    <canvas
      aria-label="Composed output preview"
      className="absolute inset-0 size-full max-w-none"
      ref={canvasRef}
      role="img"
    />
  );
}
