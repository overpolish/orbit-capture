// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useRef, useState } from "react";

import {
  nextTolerance,
  RulerTolerance,
  toleranceThreshold,
} from "./ruler-tolerance";

export function useRulerTolerance() {
  const [mode, setMode] = useState<RulerTolerance>("medium");
  const [notice, setNotice] = useState<RulerTolerance>();
  const timerRef = useRef(0);
  useEffect(
    () => () => {
      window.clearTimeout(timerRef.current);
    },
    [],
  );
  const cycle = useCallback(() => {
    const next = nextTolerance(mode);
    setMode(next);
    setNotice(next);
    window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      setNotice(undefined);
    }, 900);
  }, [mode]);
  // The threshold is a pure function of the mode, so keying refetches on it is
  // the same thing as keying on the mode itself.
  return { cycle, notice, threshold: toleranceThreshold(mode) };
}
