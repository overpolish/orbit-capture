// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

const EXPORT_PROGRESS_EVENT = "export://progress";

export type ExportPhase = "camera" | "finalizing" | "recording";

type ExportProgressEvent = {
  artifactId: number;
  phase: ExportPhase;
  progressPercent: number;
};

export function useExportProgress(artifactId?: number) {
  const [phase, setPhase] = useState<ExportPhase>("recording");
  const [progress, setProgress] = useState<number | null>(null);

  useEffect(() => {
    if (artifactId === undefined) return;

    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ExportProgressEvent>(EXPORT_PROGRESS_EVENT, ({ payload }) => {
      if (disposed || payload.artifactId !== artifactId) return;
      setPhase(payload.phase);
      // The backend weights screen and camera work and reserves the final one
      // percent for validating that both atomic renames have published.
      setProgress(Math.min(99, payload.progressPercent));
    }).then((stopListening) => {
      if (disposed) stopListening();
      else unlisten = stopListening;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [artifactId]);

  const begin = useCallback((hasMeasuredProgress: boolean) => {
    setPhase("recording");
    setProgress(hasMeasuredProgress ? 0 : null);
  }, []);
  const complete = useCallback(() => {
    setProgress(100);
  }, []);
  const reset = useCallback(() => {
    setPhase("recording");
    setProgress(null);
  }, []);

  return { begin, complete, phase, progress, reset };
}
