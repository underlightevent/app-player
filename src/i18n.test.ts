/**
 * Échoue si une clé de traduction manque dans l'une des langues,
 * ou si le code utilise une clé qui n'existe pas.
 */
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import fr from "./locales/fr.json";
import en from "./locales/en.json";

type Tree = { [k: string]: string | Tree };

function flatten(tree: Tree, prefix = ""): Map<string, string> {
  const out = new Map<string, string>();
  for (const [k, v] of Object.entries(tree)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (typeof v === "string") out.set(key, v);
    else for (const [kk, vv] of flatten(v, key)) out.set(kk, vv);
  }
  return out;
}

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) return sourceFiles(p);
    return /\.tsx?$/.test(name) && !name.endsWith(".test.ts") ? [p] : [];
  });
}

const frKeys = flatten(fr as Tree);
const enKeys = flatten(en as Tree);

describe("traductions", () => {
  it("le français et l'anglais ont exactement les mêmes clés", () => {
    const missingInEn = [...frKeys.keys()].filter((k) => !enKeys.has(k));
    const missingInFr = [...enKeys.keys()].filter((k) => !frKeys.has(k));
    expect(missingInEn, "clés absentes de en.json").toEqual([]);
    expect(missingInFr, "clés absentes de fr.json").toEqual([]);
  });

  it("aucune traduction n'est vide", () => {
    const empty = [...frKeys, ...enKeys].filter(([, v]) => v.trim() === "").map(([k]) => k);
    expect(empty).toEqual([]);
  });

  it("les variables {{…}} sont les mêmes dans les deux langues", () => {
    const vars = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();
    const diff = [...frKeys].filter(([k, v]) => JSON.stringify(vars(v)) !== JSON.stringify(vars(enKeys.get(k) ?? "")));
    expect(diff.map(([k]) => k)).toEqual([]);
  });

  it("chaque clé littérale utilisée dans le code existe", () => {
    const used = new Set<string>();
    for (const file of sourceFiles(join(__dirname))) {
      const code = readFileSync(file, "utf8");
      for (const m of code.matchAll(/\bt\(\s*"([a-zA-Z0-9_.]+)"/g)) used.add(m[1]);
    }
    expect(used.size).toBeGreaterThan(10);
    // Une clé au pluriel existe sous la forme clé_one / clé_other.
    const exists = (k: string) => frKeys.has(k) || (frKeys.has(`${k}_one`) && frKeys.has(`${k}_other`));
    const unknown = [...used].filter((k) => !exists(k));
    expect(unknown).toEqual([]);
  });

  it("les clés construites dynamiquement existent", () => {
    const views = ["library", "inbox", "review", "duplicates", "health", "quarantine", "playlists", "folders", "settings"];
    for (const v of views) expect(frKeys.has(`nav.${v}`), `nav.${v}`).toBe(true);
    for (const v of views.filter((v) => v !== "settings"))
      for (const part of ["title", "text"]) expect(frKeys.has(`views.${v}.${part}`), `views.${v}.${part}`).toBe(true);
    for (const code of ["database", "database_too_new", "database_unavailable", "io", "invalid_setting", "internal"])
      expect(frKeys.has(`errors.${code}`), `errors.${code}`).toBe(true);
    for (const lang of ["fr", "en"]) expect(frKeys.has(`languages.${lang}`)).toBe(true);
    const columns = ["cover", "rating", "mainArtist", "title", "version", "artists", "album", "year", "genre", "bpm", "key", "format", "quality", "duration", "fileName", "status"];
    for (const c of columns) expect(frKeys.has(`columns.${c}`), `columns.${c}`).toBe(true);
    for (const s of ["running", "done", "cancelled", "error"]) expect(frKeys.has(`tasks.state.${s}`)).toBe(true);
    for (const l of ["tasks.scan", "tasks.hash"]) expect(frKeys.has(l), l).toBe(true);
  });
});
