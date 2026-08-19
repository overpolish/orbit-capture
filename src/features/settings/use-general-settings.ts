// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import { getGeneralSettings } from "./api";
import { GeneralSettings } from "./types";

const SETTINGS_CHANGED_EVENT = "settings://changed";

/**
 * The general settings as they currently stand, for windows that only read
 * them: the stored values on mount, then whatever the settings window saves.
 * `null` until the first load answers.
 */
export function useGeneralSettings(): GeneralSettings | null {
  const [settings, setSettings] = useState<GeneralSettings | null>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    void getGeneralSettings().then((loaded) => {
      if (!disposed) setSettings(loaded);
    });
    void listen<GeneralSettings>(SETTINGS_CHANGED_EVENT, ({ payload }) => {
      setSettings(payload);
    }).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return settings;
}
