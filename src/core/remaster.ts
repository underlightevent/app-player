/** Remaster (phase 7) : réglages, écoute en direct avant / après, spectre, export WAV. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

export type BandKind = "lowShelf" | "peak" | "highShelf";

export interface Band {
  kind: BandKind;
  freq: number;
  gainDb: number;
  q: number;
}

export interface RemasterSettings {
  highpassHz: number;
  eqOn: boolean;
  bands: Band[];
  width: number;
  monoBassHz: number;
  compOn: boolean;
  thresholdDb: number;
  ratio: number;
  attackMs: number;
  releaseMs: number;
  makeupDb: number;
  targetLufs: number | null;
  limiterOn: boolean;
  ceilingDb: number;
  /** Coupe : début et fin gardés (secondes, au dixième ; fin 0 = jusqu'au bout). */
  startS: number;
  endS: number;
  /** Fondus en entrée et en sortie (secondes ; 0 = aucun). */
  fadeInS: number;
  fadeOutS: number;
}

/** Préréglage enregistré par l'utilisateur (sans la coupe, propre à chaque morceau). */
export interface UserPreset {
  name: string;
  settings: RemasterSettings;
}

export interface Prepared {
  beforeLufs: number;
  afterLufs: number;
  gainDb: number;
  afterPeakDb: number;
}

/** Mesures d'une version sur l'extrait comparé. */
export interface Analysis {
  bands: number[];
  side: number[];
  lufs: number;
  peakDb: number;
  rmsDb: number;
  widthPct: number;
}

/** Original et résultat sur le même extrait (45 s au milieu de la partie gardée). */
export interface Compare {
  freqs: number[];
  before: Analysis;
  after: Analysis;
  gainDb: number;
  fromS: number;
  secs: number;
}

export interface ExportResult {
  trackId: number;
  fileName: string;
  lufs: number;
  peakDb: number;
}

export const PRESETS = ["auto", "club", "warm", "bright", "punch", "soft"] as const;

export const DEFAULT_SETTINGS: RemasterSettings = {
  highpassHz: 25,
  eqOn: true,
  bands: [
    { kind: "lowShelf", freq: 90, gainDb: 0, q: 0.7 },
    { kind: "peak", freq: 250, gainDb: 0, q: 1 },
    { kind: "peak", freq: 1000, gainDb: 0, q: 1 },
    { kind: "peak", freq: 3500, gainDb: 0, q: 1 },
    { kind: "highShelf", freq: 10000, gainDb: 0, q: 0.7 },
  ],
  width: 1,
  monoBassHz: 0,
  compOn: false,
  thresholdDb: -18,
  ratio: 2,
  attackMs: 20,
  releaseMs: 200,
  makeupDb: 0,
  targetLufs: null,
  limiterOn: true,
  ceilingDb: -1,
  startS: 0,
  endS: 0,
  fadeInS: 0,
  fadeOutS: 0,
};

/** Lit la liste des préréglages de l'utilisateur (texte JSON du réglage « remaster_presets »). */
export function parseUserPresets(raw: string | undefined): UserPreset[] {
  try {
    const v = JSON.parse(raw || "[]") as UserPreset[];
    return Array.isArray(v) ? v.filter((p) => p && typeof p.name === "string" && p.settings).map((p) => ({ name: p.name, settings: { ...DEFAULT_SETTINGS, ...p.settings } })) : [];
  } catch {
    return [];
  }
}

// ---------- Aperçu navigateur ----------

export async function remasterGet(id: number): Promise<RemasterSettings | null> {
  if (!inTauri) return null;
  return invoke<RemasterSettings | null>("remaster_get", { id });
}

export async function remasterAuto(id: number, preset: string): Promise<RemasterSettings> {
  if (!inTauri) {
    const s = structuredClone(DEFAULT_SETTINGS);
    s.compOn = true;
    s.monoBassHz = 120;
    s.targetLufs = preset === "club" ? -7 : preset === "soft" ? -12 : -9;
    s.bands[0].gainDb = preset === "warm" ? 2.5 : 1;
    s.bands[4].gainDb = preset === "bright" ? 3 : 1.5;
    return s;
  }
  return invoke<RemasterSettings>("remaster_auto", { id, preset });
}

export async function remasterSpectrum(id: number, settings: RemasterSettings): Promise<Compare> {
  if (!inTauri) {
    await new Promise((r) => setTimeout(r, 300));
    const freqs = Array.from({ length: 48 }, (_, i) => Math.sqrt(30 * 600 ** (i / 48) * 30 * 600 ** ((i + 1) / 48)));
    const base = freqs.map((f, i) => -18 - i * 0.8 + Math.sin(i / 3) * 2 - (f < 40 ? 6 : 0));
    // Aperçu : effet approximatif de l'égaliseur (cloche large autour de chaque bande).
    const eq = (f: number) =>
      settings.eqOn ? settings.bands.reduce((a, b) => a + b.gainDb * Math.exp(-((Math.log2(f / b.freq) / (b.kind === "peak" ? 1 : 2)) ** 2)), 0) : 0;
    const after = freqs.map((f, i) => base[i] + eq(f) + (settings.highpassHz && f < settings.highpassHz ? -12 : 0) + 2);
    const side = (v: number[], w: number) => v.map((d, i) => d - 12 + (freqs[i] < settings.monoBassHz ? -30 : 0) + 20 * Math.log10(Math.max(w, 0.01)));
    const a = { bands: base, side: side(base, 1), lufs: -11.4, peakDb: -0.2, rmsDb: -12.5, widthPct: 42 };
    const b = { bands: after, side: side(after, settings.width), lufs: settings.targetLufs ?? -9.6, peakDb: settings.ceilingDb, rmsDb: -10.8, widthPct: 42 * settings.width };
    return { freqs, before: a, after: b, gainDb: 2, fromS: settings.startS + 60, secs: 45 };
  }
  return invoke<Compare>("remaster_spectrum", { id, settings });
}

export async function remasterLevel(id: number, settings: RemasterSettings): Promise<Prepared> {
  if (!inTauri) return { beforeLufs: -14.2, afterLufs: settings.targetLufs ?? -13.8, gainDb: 5.1, afterPeakDb: settings.ceilingDb };
  return invoke<Prepared>("remaster_level", { id, settings });
}

export async function remasterListen(id: number, settings: RemasterSettings): Promise<Prepared> {
  if (!inTauri) return remasterLevel(id, settings);
  return invoke<Prepared>("remaster_listen", { id, settings });
}

export function remasterUpdate(settings: RemasterSettings) {
  if (!inTauri) return;
  void invoke("remaster_update", { settings });
}

export function remasterAb(bypass: boolean) {
  if (!inTauri) return;
  void invoke("remaster_ab", { bypass });
}

export async function remasterExport(id: number, settings: RemasterSettings): Promise<ExportResult> {
  if (!inTauri) return { trackId: 999, fileName: "Morceau (Remaster).wav", lufs: settings.targetLufs ?? -12, peakDb: settings.ceilingDb };
  return invoke<ExportResult>("remaster_export", { id, settings });
}

/** `settings` : préréglage de l'utilisateur (appliqué tel quel, sans analyse). */
export async function remasterBatch(ids: number[], preset: string, settings: RemasterSettings | null = null): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("remaster_batch", { ids, preset, settings });
}
