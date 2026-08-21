// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import type { PixelSize } from "./pixel-analysis";

/** Extension point for ruler viewport sizing and future DPI/resize policy. */
export const rulerViewportSize = (): PixelSize => ({
  height: window.innerHeight,
  width: window.innerWidth,
});
