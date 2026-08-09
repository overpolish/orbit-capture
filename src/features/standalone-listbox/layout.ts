// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export const standaloneListboxMaxHeight = 150;
export const emptyStandaloneListboxHeight = 64;

const itemHeight = 28;
const itemGap = 8;
const listboxChrome = 10;

export const initialStandaloneListboxHeight = (itemCount: number) =>
  itemCount === 0
    ? emptyStandaloneListboxHeight
    : Math.min(
        itemCount * itemHeight +
          Math.max(itemCount - 1, 0) * itemGap +
          listboxChrome,
        standaloneListboxMaxHeight,
      );
