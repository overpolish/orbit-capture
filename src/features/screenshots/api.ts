// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";

import { Region } from "../recording-sources/types";
export type ScreenshotDestination = "export" | "clipboard" | "both";

export type ScreenshotTarget =
  | { kind: "region"; monitorId: number; region: Region }
  | { kind: "screen"; monitorId: number }
  | { kind: "window"; windowId: number };

type CaptureStillOptions = {
  destination: ScreenshotDestination;
  showCursor: boolean;
  target: ScreenshotTarget;
};

/** Resolves to the saved file's path, or null when it went to the clipboard. */
export const captureStill = ({
  destination,
  showCursor,
  target,
}: CaptureStillOptions) =>
  invoke<string | null>("capture_still", {
    destination,
    showCursor,
    target,
  });
