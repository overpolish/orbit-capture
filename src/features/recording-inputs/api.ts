// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";

export const toggleRecordingOptions = (anchorX: number) =>
  invoke<null>("toggle_recording_options", { anchorX });

export const hideRecordingOptions = () =>
  invoke<null>("hide_recording_options");
