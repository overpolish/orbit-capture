// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { isTauri } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";

import { getExportSnapshot } from "./api";
import { useExportStore } from "./store";
import { ExportSnapshot } from "./types";

const EXPORT_CHANGED_EVENT = "export://artifact";

export function ExportSync() {
  const setSnapshot = useExportStore((state) => state.setSnapshot);
  const setSnapshots = useExportStore((state) => state.setSnapshots);

  useEffect(() => {
    if (!isTauri()) return;

    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    const synchronize = async () => {
      unlisten = await listen<ExportSnapshot>(
        EXPORT_CHANGED_EVENT,
        ({ payload }) => {
          setSnapshot(payload);
        },
      );

      if (disposed) {
        unlisten();
        return;
      }

      // Every workspace at once: a webview that has just come up missed
      // whatever change events landed before it was listening.
      setSnapshots(await getExportSnapshot());
    };

    void synchronize();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [setSnapshot, setSnapshots]);

  return null;
}
