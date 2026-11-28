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
}

export interface Prepared {
  beforeLufs: number;
  afterLufs: number;
  gainDb: number;
  afterPeakDb: number;
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
};

// ---------- Aperçu navigateur ----------
const demoSpectrum = (tilt: number) => Array.from({ length: 24 }, (_, i) => -20 - i * 1.6 + tilt * (i / 23) + Math.sin(i / 2) * 2);

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

export async function remasterSpectrum(id: number, settings: RemasterSettings | null, gainDb: number): Promise<number[]> {
  if (!inTauri) return demoSpectrum(settings ? settings.bands[4].gainDb * 2 + 4 : 0).map((v) => v + (settings ? gainDb / 2 : 0));
  return invoke<number[]>("remaster_spectrum", { id, settings, gainDb });
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

export async function remasterBatch(ids: number[], preset: string): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("remaster_batch", { ids, preset });
}
