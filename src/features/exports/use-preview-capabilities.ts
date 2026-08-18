// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { useEffect, useState } from "react";

import { PreviewCapabilities, previewCapabilities } from "./api";

/**
 * What we assume when the backend cannot be asked at all (a Storybook render,
 * or a probe that failed): the DOM/canvas preview, which works everywhere.
 */
const UNSUPPORTED: PreviewCapabilities = {
  nativeRecordingPreview: false,
  nativeScreenshotPreview: false,
  nativeWorkspaceEditor: false,
};

let cached: PreviewCapabilities | undefined;
let pending: Promise<PreviewCapabilities> | undefined;

const load = () => {
  pending ??= previewCapabilities()
    .catch(() => UNSUPPORTED)
    .then((capabilities) => {
      cached = capabilities;
      return capabilities;
    });
  return pending;
};

// Start the probe as the export window's module graph loads, well before the
// first render, so callers below almost always read a resolved value and the
// preview never paints down one path and then switches to the other.
void load();

/**
 * The backend's preview capabilities, or `undefined` until the probe resolves.
 *
 * Callers must render neither preview path while this is `undefined` rather
 * than guessing: guessing "native" flashes an empty pane on platforms without
 * a backend, and guessing "fallback" mounts and tears down a decode on the
 * platforms that have one.
 */
export function usePreviewCapabilities(): PreviewCapabilities | undefined {
  const [capabilities, setCapabilities] = useState(cached);
  useEffect(() => {
    if (capabilities) return;
    let disposed = false;
    void load().then((next) => {
      if (!disposed) setCapabilities(next);
    });
    return () => {
      disposed = true;
    };
  }, [capabilities]);
  return capabilities;
}
