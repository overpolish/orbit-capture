// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { synchronizeRecordingSourceStore } from "./store";

export function RecordingSourceSync() {
  useEffect(() => {
    window.addEventListener("storage", synchronizeRecordingSourceStore);

    return () => {
      window.removeEventListener("storage", synchronizeRecordingSourceStore);
    };
  }, []);

  return null;
}
