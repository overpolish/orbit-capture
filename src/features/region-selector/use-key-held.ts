// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

/** Whether a modifier or key is being held down anywhere in the window. */
export const useKeyHeld = (key: string) => {
  const [isHeld, setIsHeld] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === key) setIsHeld(true);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === key) setIsHeld(false);
    };
    // The release lands wherever focus went, so a hold that outlives the
    // window would never end.
    const onBlur = () => {
      setIsHeld(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", onBlur);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [key]);

  return isHeld;
};
