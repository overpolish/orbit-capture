// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import {
  getRulerBoxes,
  getRulerGradients,
  RulerComponentBox,
  RulerGradientsMeta,
} from "./api";
import { GradientField } from "./gradient-field";

const fieldFrom = (
  buffer: ArrayBuffer,
  { height, width }: RulerGradientsMeta,
): GradientField => {
  const plane = width * height;
  const bytes = new Uint8Array(buffer);
  const gx = bytes.subarray(0, plane);
  const gy = bytes.subarray(plane, plane * 2);
  const colSum = new Float32Array(width);
  const rowSum = new Float32Array(height);
  for (let y = 0; y < height; y += 1) {
    const row = y * width;
    let rowTotal = 0;
    for (let x = 0; x < width; x += 1) {
      colSum[x] += gx[row + x];
      rowTotal += gy[row + x];
    }
    rowSum[y] = rowTotal;
  }
  return { colSum, gx, gy, height, rowSum, width };
};

/**
 * Gradients are threshold-independent, so they are fetched exactly once per
 * monitor; only the component boxes are refetched when tolerance changes.
 */
export function useRulerAnalysis({
  monitorId,
  threshold,
}: {
  monitorId: number;
  threshold: number;
}) {
  const [field, setField] = useState<GradientField>();
  const [boxes, setBoxes] = useState<readonly RulerComponentBox[]>([]);

  useEffect(() => {
    let disposed = false;
    let meta: RulerGradientsMeta | undefined;
    let buffer: ArrayBuffer | undefined;
    const commit = () => {
      if (!disposed && meta && buffer) setField(fieldFrom(buffer, meta));
    };
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (message) => {
      buffer = message;
      commit();
    };
    getRulerGradients(monitorId, channel)
      .then((result) => {
        meta = result;
        commit();
      })
      .catch((error: unknown) => {
        console.error("Could not load the ruler gradients", error);
      });
    return () => {
      disposed = true;
    };
  }, [monitorId]);

  useEffect(() => {
    let disposed = false;
    getRulerBoxes(monitorId, threshold)
      .then((result) => {
        if (!disposed) setBoxes(result);
      })
      .catch((error: unknown) => {
        console.error("Could not load the ruler boxes", error);
      });
    return () => {
      disposed = true;
    };
  }, [monitorId, threshold]);

  return { boxes, field };
}
