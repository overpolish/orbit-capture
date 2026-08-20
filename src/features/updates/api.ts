// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { invoke } from "@tauri-apps/api/core";

export const showUpdatePrompt = () => invoke<null>("show_update_prompt");

export const hideUpdatePrompt = () => invoke<null>("hide_update_prompt");
