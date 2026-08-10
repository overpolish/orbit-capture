// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type PermissionKind =
  "accessibility" | "camera" | "microphone" | "screenRecording";

export type PermissionStatus = {
  canRequest: boolean;
  granted: boolean;
};

export type PermissionSnapshot = Record<PermissionKind, PermissionStatus>;

export const initialPermissionSnapshot: PermissionSnapshot = {
  accessibility: { canRequest: true, granted: false },
  camera: { canRequest: true, granted: false },
  microphone: { canRequest: true, granted: false },
  screenRecording: { canRequest: true, granted: false },
};
