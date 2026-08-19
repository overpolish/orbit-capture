// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { getCurrentWindow } from "@tauri-apps/api/window";

import { ExportKind } from "./types";

const LABELS: Record<string, ExportKind> = {
  "export-recording": "recording",
  "export-screenshot": "screenshot",
};

let resolved: ExportKind | null | undefined;

/**
 * Which workspace this webview is showing, read off its own window label.
 *
 * Rust derives the same thing from the calling window, so no `invoke` has to
 * carry it. Memoized because a window's label never changes, and this is read
 * on every render of the export window.
 */
export function currentExportKind(): ExportKind | null {
  resolved ??= LABELS[getCurrentWindow().label] ?? null;

  return resolved;
}
