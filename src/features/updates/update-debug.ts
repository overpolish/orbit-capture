// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

const enabled =
  import.meta.env.DEV || import.meta.env.VITE_SCREENWIDE_UPDATE_DEBUG === "1";

export const updateDebug = (
  message: string,
  details?: Record<string, unknown>,
) => {
  if (!enabled) return;
  if (details) {
    console.info(`[Screenwide updater] ${message}`, details);
  } else {
    console.info(`[Screenwide updater] ${message}`);
  }
};
