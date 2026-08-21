// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { Channel, invoke } from "@tauri-apps/api/core";

export type RulerSnapshot = {
  height: number;
  scale: number;
  width: number;
};

export const cancelRuler = () => invoke<null>("cancel_ruler");

export const copyRulerValue = (value: string) =>
  invoke<null>("copy_ruler_value", { value });

export type RulerComponentBox = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export type RulerGradientsMeta = {
  height: number;
  width: number;
};

export const getRulerBoxes = (monitorId: number, threshold: number) =>
  invoke<RulerComponentBox[]>("get_ruler_boxes", { monitorId, threshold });

// The channel receives one buffer: the horizontal gradient plane followed by
// the vertical plane, each width * height bytes.
export const getRulerGradients = (
  monitorId: number,
  channel: Channel<ArrayBuffer>,
) => invoke<RulerGradientsMeta>("get_ruler_gradients", { channel, monitorId });

export const getRulerSnapshot = (
  monitorId: number,
  channel: Channel<ArrayBuffer>,
) => invoke<RulerSnapshot>("get_ruler_snapshot", { channel, monitorId });

export const setRulerScreenshotMode = (active: boolean) =>
  invoke<null>("set_ruler_screenshot_mode", { active });
