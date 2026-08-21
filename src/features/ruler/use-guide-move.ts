// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef, useState } from "react";

type Gesture = { id: number; recorded: boolean };

/**
 * Picking a halo-selected guide up and carrying it. The live gesture lives in a
 * ref because the pointer handlers have to read it synchronously, while
 * `activeId` drives the render gates the drag shares with guide placement.
 *
 * `recorded` is the caller's flag: history is taken on the first movement, so a
 * click that never moves the guide leaves the undo stack alone.
 */
export function useGuideMove() {
  const [activeId, setActiveId] = useState<number>();
  const gestureRef = useRef<Gesture | null>(null);

  const begin = useCallback((id: number) => {
    gestureRef.current = { id, recorded: false };
    setActiveId(id);
  }, []);

  const end = useCallback(() => {
    if (!gestureRef.current) return false;
    gestureRef.current = null;
    setActiveId(undefined);
    return true;
  }, []);

  const gesture = useCallback(() => gestureRef.current, []);

  return { activeId, begin, end, gesture };
}
