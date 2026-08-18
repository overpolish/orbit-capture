// SPDX-FileCopyrightText: 2026 overpolish
// SPDX-License-Identifier: GPL-3.0-or-later

const systemTheme = window.matchMedia("(prefers-color-scheme: dark)");

const applySystemTheme = ({ matches }: Pick<MediaQueryList, "matches">) => {
  const changed =
    document.documentElement.classList.contains("dark") !== matches;
  document.documentElement.classList.toggle("dark", matches);
  if (changed) window.dispatchEvent(new Event("screenwide-theme-changed"));
};

export const synchronizeSystemTheme = () => {
  applySystemTheme(systemTheme);
  systemTheme.addEventListener("change", applySystemTheme);

  return () => {
    systemTheme.removeEventListener("change", applySystemTheme);
  };
};
