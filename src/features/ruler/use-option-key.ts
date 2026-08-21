// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

/**
 * True while Alt/Option is held; macOS reports Option as `Alt`. Losing focus
 * clears the flag, because the keyup then lands somewhere else and the
 * modifier would otherwise stick forever.
 */
export function useOptionKey() {
  const [held, setHeld] = useState(false);

  useEffect(() => {
    const clear = () => {
      setHeld(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Alt") setHeld(true);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === "Alt") setHeld(false);
    };
    window.addEventListener("blur", clear);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("blur", clear);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, []);

  return held;
}
