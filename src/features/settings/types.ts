// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type ShortcutAction =
  | "toggleRecordingBar"
  | "startStopRecording"
  | "pauseResumeRecording"
  | "takeScreenshot"
  | "recognizeText";

type ShortcutBinding = {
  action: ShortcutAction;
  shortcut: string | null;
};

export type ShortcutSettings = {
  bindings: ShortcutBinding[];
};

export type GeneralSettings = {
  captureScreenshotOnDraw: boolean;
  launchAtLogin: boolean;
  openLocationAfterExport: boolean;
  recordScreenwideWindows: boolean;
  recordingCountdownSeconds: 0 | 3 | 5;
  recordingDirectory: string | null;
  screenshotDirectory: string | null;
  showRecordingBarOnLaunch: boolean;
  showRecordingConfidenceChecks: boolean;
};
