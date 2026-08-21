// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

const SCREENSHOT_MODE_EVENT = "ruler://screenshot-mode";

export function useRulerScreenshotMode() {
  const [active, setActive] = useState(false);
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    void listen<boolean>(SCREENSHOT_MODE_EVENT, ({ payload }) => {
      setActive(payload);
    }).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return active;
}
