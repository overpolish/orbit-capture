// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export const permissionKinds = [
  "accessibility",
  "screenRecording",
  "camera",
  "microphone",
] as const;

export type PermissionKind = (typeof permissionKinds)[number];

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
