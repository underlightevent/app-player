export type ThemeSetting = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export function isThemeSetting(v: unknown): v is ThemeSetting {
  return v === "light" || v === "dark" || v === "system";
}

export function resolveTheme(setting: ThemeSetting, systemPrefersDark: boolean): ResolvedTheme {
  if (setting === "system") return systemPrefersDark ? "dark" : "light";
  return setting;
}

export function systemPrefersDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export function applyTheme(theme: ResolvedTheme) {
  document.documentElement.dataset.theme = theme;
}
