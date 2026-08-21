// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef, useState } from "react";

import { Bounds, orderedBounds, Point } from "./pixel-analysis";

const DRAG_THRESHOLD = 4;

export function useBoxDrag(onFinish: (bounds: Bounds) => void) {
  const [draft, setDraft] = useState<Bounds>();
  const pendingRef = useRef<{ screen: Point; world: Point } | null>(null);
  const startRef = useRef<Point | null>(null);

  const begin = useCallback((screen: Point, world: Point) => {
    pendingRef.current = { screen, world };
  }, []);
  const move = useCallback((screen: Point, world: Point) => {
    const start = startRef.current;
    if (start) {
      setDraft(orderedBounds(start, world));
      return;
    }
    const pending = pendingRef.current;
    if (
      !pending ||
      Math.hypot(screen.x - pending.screen.x, screen.y - pending.screen.y) <
        DRAG_THRESHOLD
    )
      return;
    startRef.current = pending.world;
    setDraft(orderedBounds(pending.world, world));
  }, []);
  const cancel = useCallback(() => {
    pendingRef.current = null;
    startRef.current = null;
    setDraft(undefined);
  }, []);
  const finish = useCallback(
    (world: Point) => {
      const start = startRef.current;
      cancel();
      if (start) onFinish(orderedBounds(start, world));
    },
    [cancel, onFinish],
  );
  return {
    begin,
    cancel,
    draft,
    finish,
    isActive: () => startRef.current !== null,
    move,
  };
}
