// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useRef } from "react";

import { CameraOverlaySettings } from "../types";

export const useCameraOverlayHistory = ({
  enabled,
  onChange,
  resetKey,
  settings,
}: {
  enabled: boolean;
  resetKey: string | number;
  settings: CameraOverlaySettings;
  onChange?: (settings: CameraOverlaySettings) => void;
}) => {
  const currentRef = useRef(settings);
  const onChangeRef = useRef(onChange);
  currentRef.current = settings;
  onChangeRef.current = onChange;

  const apply = useCallback((next: CameraOverlaySettings) => {
    currentRef.current = next;
    onChangeRef.current?.(next);
  }, []);

  // The export window owns undo history. These remain as gesture boundaries
  // for the camera viewport API, while changes flow into that shared stack.
  void enabled;
  void resetKey;

  return {
    beginGesture: () => undefined,
    change: apply,
    endGesture: () => undefined,
  };
};
