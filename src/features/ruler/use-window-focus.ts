// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

/**
 * Whether this ruler window holds key focus. Only the focused window receives
 * mouse events, so an unfocused window's cursor-following readouts are stale by
 * definition and hide until focus returns — the multi-monitor hand-off then
 * reads as one smooth transition instead of two half-alive overlays.
 */
export function useWindowFocus() {
  const [focused, setFocused] = useState(() => document.hasFocus());
  useEffect(() => {
    const onFocus = () => {
      setFocused(true);
    };
    const onBlur = () => {
      setFocused(false);
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  return focused;
}
