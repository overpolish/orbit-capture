// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useCallback, useEffect, useState } from "react";

import { getExportPreview } from "./api";

export function useExportPreviewImage(
  artifactId: number | undefined,
  loadFullResolutionInitially = false,
  itemId?: number | null,
) {
  const resourceKey = `${artifactId?.toString() ?? "none"}:${itemId?.toString() ?? "none"}`;
  const [preview, setPreview] = useState<{
    key: string;
    url: string;
  } | null>(null);
  const [fullPreview, setFullPreview] = useState<{
    key: string;
    url: string;
  } | null>(null);

  useEffect(() => {
    if (artifactId === undefined) return;

    let url: string | undefined;
    let disposed = false;

    void getExportPreview(loadFullResolutionInitially, itemId ?? undefined)
      .then((bytes) => {
        if (disposed) return;
        url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setPreview({ key: resourceKey, url });
      })
      .catch((cause: unknown) => {
        console.error("Could not load the export preview", cause);
      });

    return () => {
      disposed = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [artifactId, itemId, loadFullResolutionInitially, resourceKey]);

  useEffect(() => {
    if (!fullPreview) return;
    return () => {
      URL.revokeObjectURL(fullPreview.url);
    };
  }, [fullPreview]);

  const loadFullPreview = useCallback(() => {
    if (
      loadFullResolutionInitially ||
      (fullPreview && fullPreview.key === resourceKey)
    )
      return;

    void getExportPreview(true, itemId ?? undefined)
      .then((bytes) => {
        setFullPreview({
          key: resourceKey,
          url: URL.createObjectURL(new Blob([bytes], { type: "image/png" })),
        });
      })
      .catch((cause: unknown) => {
        console.error("Could not load the full-resolution preview", cause);
      });
  }, [fullPreview, itemId, loadFullResolutionInitially, resourceKey]);

  const previewUrl = preview?.key === resourceKey ? preview.url : null;
  const fullPreviewUrl =
    fullPreview?.key === resourceKey ? fullPreview.url : null;

  return {
    loadFullPreview,
    previewUrl: fullPreviewUrl ?? previewUrl,
  };
}
