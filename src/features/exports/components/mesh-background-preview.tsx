// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

import { renderMeshBackgroundPreview } from "../api";
import { MeshGradientPoint } from "../screenshot-background";

export function MeshBackgroundPreview({
  colors,
  points,
  seed,
  size,
  warpPercent,
}: {
  colors: string[];
  points: MeshGradientPoint[];
  seed: number;
  size: { height: number; width: number };
  warpPercent: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    let disposed = false;
    const channel = new Channel<ArrayBuffer>();
    const requestId = ++requestRef.current;
    channel.onmessage = (payload) => {
      if (disposed) return;
      if (payload.byteLength < 12) return;
      const header = new DataView(payload, 0, 12);
      const receivedRequestId = header.getUint32(0, true);
      const width = header.getUint32(4, true);
      const height = header.getUint32(8, true);
      if (
        receivedRequestId !== requestRef.current ||
        width === 0 ||
        height === 0
      )
        return;
      const canvas = canvasRef.current;
      if (!canvas) return;
      void createImageBitmap(
        new Blob([payload.slice(12)], { type: "image/jpeg" }),
      ).then((bitmap) => {
        if (disposed || receivedRequestId !== requestRef.current) {
          bitmap.close();
          return;
        }
        const current = canvasRef.current;
        if (!current) {
          bitmap.close();
          return;
        }
        if (current.width !== width) current.width = width;
        if (current.height !== height) current.height = height;
        current.getContext("2d")?.drawImage(bitmap, 0, 0, width, height);
        bitmap.close();
      });
    };
    void renderMeshBackgroundPreview({
      channel,
      colors,
      height: size.height,
      points,
      requestId,
      seed,
      warpPercent,
      width: size.width,
    }).catch((cause: unknown) => {
      if (!disposed)
        console.error("Could not render the mesh background preview", cause);
    });
    return () => {
      disposed = true;
      requestRef.current += 1;
    };
  }, [colors, points, seed, size.height, size.width, warpPercent]);

  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 size-full overflow-visible rounded-none bg-transparent"
    >
      <canvas
        className="absolute inset-0 block size-full max-w-none rounded-none"
        ref={canvasRef}
        style={{ borderRadius: 0 }}
      />
    </div>
  );
}
