// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect } from "react";

import { synchronizeRecordingInputStore } from "./store";

export function RecordingInputSync() {
  useEffect(() => {
    window.addEventListener("storage", synchronizeRecordingInputStore);

    return () => {
      window.removeEventListener("storage", synchronizeRecordingInputStore);
    };
  }, []);

  return null;
}
