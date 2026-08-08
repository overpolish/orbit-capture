import { invoke } from "@tauri-apps/api/core";

import { Region } from "../recording-sources/types";

export type ScreenshotTarget =
  | { kind: "region"; monitorId: number; region: Region }
  | { kind: "screen"; monitorId: number }
  | { kind: "window"; windowId: number };

type CaptureStillOptions = {
  showCursor: boolean;
  target: ScreenshotTarget;
  toClipboard: boolean;
};

/** Resolves to the saved file's path, or null when it went to the clipboard. */
export const captureStill = ({
  showCursor,
  target,
  toClipboard,
}: CaptureStillOptions) =>
  invoke<string | null>("capture_still", {
    showCursor,
    target,
    toClipboard,
  });
