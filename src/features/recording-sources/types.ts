// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type MonitorDetails = {
  id: number;
  isBuiltin: boolean;
  isPrimary: boolean;
  layoutPosition: { x: number; y: number };
  layoutSize: { height: number; width: number };
  name: string;
  physicalPosition: { x: number; y: number };
  physicalSize: { height: number; width: number };
  position: { x: number; y: number };
  scaleFactor: number;
  size: { height: number; width: number };
};

export type Region = {
  position: { x: number; y: number };
  size: { height: number; width: number };
};

export type RecordingMode = "screen" | "region" | "window" | "camera" | "audio";

export type SelectorPlacement = "above" | "below";

export type WindowDetails = {
  appIconPath: string | null;
  appName: string;
  id: number;
  pid: number;
  position: { x: number; y: number };
  size: { height: number; width: number };
  thumbnailPath: string | null;
  title: string;
};
