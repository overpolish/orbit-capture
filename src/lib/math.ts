// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

export type Range = {
  max: number;
  min: number;
};

export const randomInRange = ({ max, min }: Range) =>
  Math.random() * (max - min) + min;
