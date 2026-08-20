// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { getTextRecognitionSnapshot, TextRecognitionSnapshot } from "./api";

export function FrozenMonitorSnapshot({ monitorId }: { monitorId: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [snapshot, setSnapshot] = useState<
    TextRecognitionSnapshot & { pixels: ArrayBuffer }
  >();

  useEffect(() => {
    let disposed = false;
    let metadata: TextRecognitionSnapshot | undefined;
    let pixels: ArrayBuffer | undefined;
    const commit = () => {
      if (!disposed && metadata && pixels) {
        setSnapshot({ ...metadata, pixels });
      }
    };
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (message) => {
      pixels = message;
      commit();
    };
    void getTextRecognitionSnapshot(monitorId, channel)
      .then((result) => {
        metadata = result;
        commit();
      })
      .catch((reason: unknown) => {
        console.error("Could not load the frozen OCR monitor image", reason);
      });
    return () => {
      disposed = true;
    };
  }, [monitorId]);

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !snapshot) return;
    canvas.width = snapshot.width;
    canvas.height = snapshot.height;
    canvas
      .getContext("2d")
      ?.putImageData(
        new ImageData(
          new Uint8ClampedArray(snapshot.pixels),
          snapshot.width,
          snapshot.height,
        ),
        0,
        0,
      );
  }, [snapshot]);

  return (
    <canvas
      className="pointer-events-none absolute inset-0 size-full"
      ref={canvasRef}
    />
  );
}
