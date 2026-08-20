// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export const capturedTextImageUrl = (bytes: number[]) =>
  URL.createObjectURL(
    new Blob([Uint8Array.from(bytes)], { type: "image/png" }),
  );
