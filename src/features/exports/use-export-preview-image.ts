// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useState } from "react";

import { getExportPreview } from "./api";

export function useExportPreviewImage(artifactId: number | undefined) {
  const [previewUrl, setPreviewUrl] = useState<string | null>(null);
  const [fullPreviewUrl, setFullPreviewUrl] = useState<string | null>(null);

  useEffect(() => {
    if (artifactId === undefined) return;

    let url: string | undefined;
    let disposed = false;

    void getExportPreview()
      .then((bytes) => {
        if (disposed) return;
        url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setPreviewUrl(url);
      })
      .catch((cause: unknown) => {
        console.error("Could not load the export preview", cause);
      });

    return () => {
      disposed = true;
      if (url) URL.revokeObjectURL(url);
      setPreviewUrl(null);
      setFullPreviewUrl(null);
    };
  }, [artifactId]);

  useEffect(() => {
    if (!fullPreviewUrl) return;
    return () => {
      URL.revokeObjectURL(fullPreviewUrl);
    };
  }, [fullPreviewUrl]);

  const loadFullPreview = useCallback(() => {
    if (fullPreviewUrl) return;

    void getExportPreview(true)
      .then((bytes) => {
        setFullPreviewUrl(
          URL.createObjectURL(new Blob([bytes], { type: "image/png" })),
        );
      })
      .catch((cause: unknown) => {
        console.error("Could not load the full-resolution preview", cause);
      });
  }, [fullPreviewUrl]);

  return {
    loadFullPreview,
    previewUrl: fullPreviewUrl ?? previewUrl,
  };
}
