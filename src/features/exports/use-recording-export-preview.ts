// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

import { getRecordingPreview } from "./api";
import { RecordingPreview } from "./types";

export function useRecordingExportPreview({
  artifactId,
  shouldPrepare,
}: {
  artifactId: number | undefined;
  shouldPrepare: boolean;
}) {
  const [state, setState] = useState<{
    artifactId: number;
    error: string | null;
    preview: RecordingPreview | null;
  } | null>(null);

  useEffect(() => {
    if (!shouldPrepare || artifactId === undefined) return;

    let disposed = false;
    void getRecordingPreview(artifactId)
      .then((preview) => {
        if (!disposed) {
          setState({ artifactId, error: null, preview });
        }
      })
      .catch((cause: unknown) => {
        if (disposed) return;
        console.error("Could not prepare the recording preview", cause);
        setState({
          artifactId,
          error: cause instanceof Error ? cause.message : String(cause),
          preview: null,
        });
      });

    return () => {
      disposed = true;
    };
  }, [artifactId, shouldPrepare]);

  const current =
    shouldPrepare && state?.artifactId === artifactId ? state : null;
  return {
    error: current?.error ?? null,
    isPreparing: shouldPrepare && current === null,
    preview: current?.preview ?? null,
  };
}
