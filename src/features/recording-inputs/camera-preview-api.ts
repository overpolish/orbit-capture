// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import { CameraResolution } from "./types";

/** Only the fields the native preview actually consumes. */
export type CameraPreviewMode = Pick<
  CameraResolution,
  "fps" | "height" | "width"
>;

export const startCameraPreview = async (
  deviceId: string,
  mode: CameraPreviewMode,
  channel: Channel<ArrayBuffer>,
) => {
  await invoke("start_camera_preview", {
    channel,
    deviceId,
    fps: mode.fps,
    height: mode.height,
    width: mode.width,
  });
};

export const stopCameraPreview = async () => {
  await invoke("stop_camera_preview");
};
