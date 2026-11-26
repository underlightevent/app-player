import { describe, expect, it } from "vitest";
import { isThemeSetting, resolveTheme } from "./theme";
import { isBrowserShortcut } from "./shortcuts";

describe("thème", () => {
  it("clair et sombre sont gardés tels quels", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });
  it("« Suivre le système » suit le système", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
  it("valeur inconnue refusée", () => {
    expect(isThemeSetting("rose")).toBe(false);
    expect(isThemeSetting("system")).toBe(true);
  });
});

describe("raccourcis du navigateur désactivés", () => {
  const k = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    ...mods,
  });
  it("rechargement et zoom bloqués", () => {
    expect(isBrowserShortcut(k("F5"))).toBe(true);
    expect(isBrowserShortcut(k("r", { ctrlKey: true }))).toBe(true);
    expect(isBrowserShortcut(k("r", { metaKey: true }))).toBe(true);
    expect(isBrowserShortcut(k("+", { ctrlKey: true }))).toBe(true);
    expect(isBrowserShortcut(k("0", { metaKey: true }))).toBe(true);
  });
  it("frappe normale et raccourcis de l'application laissés passer", () => {
    expect(isBrowserShortcut(k("r"))).toBe(false);
    expect(isBrowserShortcut(k("k", { ctrlKey: true }))).toBe(false);
    expect(isBrowserShortcut(k("i", { ctrlKey: true }))).toBe(false);
    expect(isBrowserShortcut(k(" "))).toBe(false);
  });
});
