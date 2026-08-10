// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { isTauri } from "@tauri-apps/api/core";
import {
  currentMonitor,
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";
import { useCallback } from "react";

const DEFAULT_WINDOW_WIDTH = 560;
const WINDOW_PADDING = 48;
const WINDOW_MARGIN = 24;

export function useExportWindowSize(width = DEFAULT_WINDOW_WIDTH) {
  return useCallback(
    (height: number) => {
      if (!isTauri()) return;

      const desiredHeight = Math.ceil(height) + WINDOW_PADDING;
      const target = getCurrentWindow();

      void (async () => {
        let monitor = null;
        try {
          monitor = await currentMonitor();
        } catch (cause) {
          console.error("Could not read the current monitor", cause);
        }

        const availableHeight = monitor
          ? monitor.workArea.size.toLogical(monitor.scaleFactor).height -
            WINDOW_MARGIN * 2
          : desiredHeight;
        await target.setSize(
          new LogicalSize(width, Math.min(desiredHeight, availableHeight)),
        );

        if (!monitor) return;

        const position = await target.outerPosition();
        const size = await target.outerSize();
        const margin = Math.round(WINDOW_MARGIN * monitor.scaleFactor);
        const minimumY = monitor.workArea.position.y + margin;
        const maximumY = Math.max(
          minimumY,
          monitor.workArea.position.y +
            monitor.workArea.size.height -
            margin -
            size.height,
        );
        const y = Math.min(Math.max(position.y, minimumY), maximumY);
        if (y !== position.y) {
          await target.setPosition(new PhysicalPosition(position.x, y));
        }
      })();
    },
    [width],
  );
}
