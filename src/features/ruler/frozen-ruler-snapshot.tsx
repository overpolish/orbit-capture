// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { getRulerSnapshot, RulerSnapshot } from "./api";
import { PixelSnapshot } from "./pixel-analysis";

export function FrozenRulerSnapshot({
  monitorId,
  onLoad,
}: {
  monitorId: number;
  onLoad: (snapshot: PixelSnapshot) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [snapshot, setSnapshot] = useState<
    RulerSnapshot & { pixels: ArrayBuffer }
  >();

  useEffect(() => {
    let disposed = false;
    let metadata: RulerSnapshot | undefined;
    let pixels: ArrayBuffer | undefined;
    const commit = () => {
      if (!disposed && metadata && pixels) setSnapshot({ ...metadata, pixels });
    };
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (message) => {
      pixels = message;
      commit();
    };
    void getRulerSnapshot(monitorId, channel).then((result) => {
      metadata = result;
      commit();
    });
    return () => {
      disposed = true;
    };
  }, [monitorId]);

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !snapshot) return;
    const pixels = new Uint8ClampedArray(snapshot.pixels);
    canvas.width = snapshot.width;
    canvas.height = snapshot.height;
    canvas
      .getContext("2d")
      ?.putImageData(
        new ImageData(pixels, snapshot.width, snapshot.height),
        0,
        0,
      );
    onLoad({ height: snapshot.height, pixels, width: snapshot.width });
  }, [onLoad, snapshot]);

  return (
    <canvas
      className="pointer-events-none absolute inset-0 size-full"
      ref={canvasRef}
    />
  );
}
