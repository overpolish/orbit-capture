// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useState } from "react";

/** The view aids a hotkey can flip: crosshair, centerlines, detected boxes. */
export function useRulerViewToggles() {
  const [crosshair, setCrosshair] = useState(false);
  const [centerlines, setCenterlines] = useState(true);
  const [detectedBoxes, setDetectedBoxes] = useState(false);
  const toggleCrosshair = useCallback(() => {
    setCrosshair((current) => !current);
  }, []);
  const toggleCenterlines = useCallback(() => {
    setCenterlines((current) => !current);
  }, []);
  const toggleDetectedBoxes = useCallback(() => {
    setDetectedBoxes((current) => !current);
  }, []);
  return {
    centerlines,
    crosshair,
    detectedBoxes,
    toggleCenterlines,
    toggleCrosshair,
    toggleDetectedBoxes,
  };
}
