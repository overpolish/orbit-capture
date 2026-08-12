// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";

import { GeneralSettings, ShortcutAction, ShortcutSettings } from "./types";

export const getShortcutSettings = () =>
  invoke<ShortcutSettings>("get_shortcut_settings");

export const setShortcutBinding = (
  action: ShortcutAction,
  shortcut: string | null,
) => invoke<ShortcutSettings>("set_shortcut_binding", { action, shortcut });

export const beginShortcutCapture = () =>
  invoke<null>("begin_shortcut_capture");

export const endShortcutCapture = () => invoke<null>("end_shortcut_capture");

export const hideSettings = () => invoke<null>("hide_settings");

export const getGeneralSettings = () =>
  invoke<GeneralSettings>("get_general_settings");

export const setGeneralSettings = (settings: GeneralSettings) =>
  invoke<GeneralSettings>("set_general_settings", { settings });

export const browseDefaultLocation = (kind: "recording" | "screenshot") =>
  invoke<string | null>("browse_default_location", { kind });
