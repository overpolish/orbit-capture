// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { RefObject, useCallback, useEffect, useRef, useState } from "react";

import { copyRulerValue } from "./api";
import { colorAt, PixelSnapshot, Point, screenToPixel } from "./pixel-analysis";
import { Measurement } from "./ruler-types";
import { rulerViewportSize } from "./ruler-viewport-size";

const FLASH_MS = 900;

export function useRulerClipboard({
  cursorRef,
  measurements,
  snapshot,
}: {
  cursorRef: RefObject<Point | undefined>;
  measurements: readonly Measurement[];
  snapshot: PixelSnapshot | undefined;
}) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef(0);
  useEffect(
    () => () => {
      window.clearTimeout(timerRef.current);
    },
    [],
  );
  const flash = useCallback(() => {
    setCopied(true);
    window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setCopied(false);
    }, FLASH_MS);
  }, []);

  const copyColor = useCallback(() => {
    const cursor = cursorRef.current;
    if (!snapshot || !cursor) return;
    const { hex } = colorAt(
      snapshot,
      screenToPixel(cursor, snapshot, rulerViewportSize()),
    );
    void copyRulerValue(hex).then(flash);
  }, [cursorRef, flash, snapshot]);

  const copyLatestMeasurement = useCallback(() => {
    if (measurements.length === 0) return;
    const latest = measurements[measurements.length - 1];
    const width = Math.round(latest.width);
    const height = Math.round(latest.height);
    const value =
      latest.height < 8
        ? `${String(width)} px`
        : latest.width < 8
          ? `${String(height)} px`
          : `${String(width)} × ${String(height)} px`;
    void copyRulerValue(value).then(flash);
  }, [flash, measurements]);

  return { copied, copyColor, copyLatestMeasurement };
}
