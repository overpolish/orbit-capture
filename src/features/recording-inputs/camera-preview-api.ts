// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

export const startCameraPreview = async (
  deviceId: string,
  channel: Channel<ArrayBuffer>,
) => {
  await invoke("start_camera_preview", { channel, deviceId });
};

export const stopCameraPreview = async () => {
  await invoke("stop_camera_preview");
};
