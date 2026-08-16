// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

import { ScreenshotWorkspaceOutputSettings } from "../screenshot-output";

export const moveScreenshotLayer = ({
  direction,
  itemId,
  settings,
}: {
  direction: "backward" | "forward";
  itemId: number;
  settings: ScreenshotWorkspaceOutputSettings;
}) => {
  const index = settings.items.findIndex((item) => item.id === itemId);
  const nextIndex = direction === "forward" ? index + 1 : index - 1;
  if (index === -1 || nextIndex < 0 || nextIndex >= settings.items.length)
    return settings;
  const items = [...settings.items];
  [items[index], items[nextIndex]] = [items[nextIndex], items[index]];
  return { ...settings, items };
};

export const deleteScreenshotLayer = ({
  itemId,
  settings,
}: {
  itemId: number;
  settings: ScreenshotWorkspaceOutputSettings;
}) => {
  if (settings.items.length <= 1) return null;
  const index = settings.items.findIndex((item) => item.id === itemId);
  if (index === -1) return null;
  const items = settings.items.filter((item) => item.id !== itemId);
  return {
    nextSelectedItemId: items[Math.min(index, items.length - 1)]?.id ?? null,
    settings: { ...settings, items },
  };
};
