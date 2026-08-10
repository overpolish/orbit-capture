// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

import { CameraResolution } from "./types";

export const startCameraPreview = async (
  deviceId: string,
  mode: CameraResolution,
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
