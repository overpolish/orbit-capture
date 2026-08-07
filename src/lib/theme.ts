const systemTheme = window.matchMedia("(prefers-color-scheme: dark)");

const applySystemTheme = ({ matches }: Pick<MediaQueryList, "matches">) => {
  document.documentElement.classList.toggle("dark", matches);
};

export const synchronizeSystemTheme = () => {
  applySystemTheme(systemTheme);
  systemTheme.addEventListener("change", applySystemTheme);

  return () => {
    systemTheme.removeEventListener("change", applySystemTheme);
  };
};
