/** Phase 8 : structure des morceaux, similaires, générateur de set, essai d'enchaînement,
 * DJ edit, séparation des pistes, détection des voix, contrôle de clé USB. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

export interface Section {
  kind: "intro" | "build" | "drop" | "break" | "outro" | "main";
  startMs: number;
  endMs: number;
  startBar: number;
  bars: number;
}

export interface Cue {
  /** 0 = A, 1 = B… ; null = repère mémoire. */
  hot: number | null;
  name: string;
  ms: number;
}

export interface Structure {
  bpm: number;
  firstBeatMs: number;
  bars: number;
  sections: Section[];
  cues: Cue[];
  leadSilenceMs: number;
  tailSilenceMs: number;
  durationMs: number;
  timbre: number[];
  confidence: number;
}

export interface Similar {
  id: number;
  score: number;
  keyCompat: string | null;
  bpmDiff: number | null;
  sound: number | null;
}

export type Curve = "rise" | "plateau" | "wave" | "fall";

export interface SetPlan {
  minutes: number;
  curve: Curve;
  bpmMin: number | null;
  bpmMax: number | null;
  pool: number[];
  start: number | null;
}

export interface TransitionPlan {
  aFromMs: number;
  mixAtMs: number;
  mixMs: number;
  bFromMs: number;
  bSpeed: number;
  totalMs: number;
  bars: number;
}

export interface EditPlan {
  introBars: number;
  outroBars: number;
  loopBars: number;
}

export interface EditResult {
  trackId: number;
  fileName: string;
  durationMs: number;
}

export interface StemsStatus {
  supported: boolean;
  installed: boolean;
  downloadBytes: number;
}

export type StemsKind = "instrumental" | "acapella" | "both" | "all";

export type UsbProfile = "old" | "nxs2" | "cdj3000";

export interface UsbIssue {
  path: string;
  kind: "unreadable" | "format" | "rate" | "depth" | "float" | "path_long" | "big_file" | "cover_heavy";
  detail: string;
}

export interface UsbReport {
  filesystem: string | null;
  filesystemOk: boolean | null;
  files: number;
  ok: number;
  issues: UsbIssue[];
  rekordboxExport: boolean;
  bytes: number;
}

// ---------- Aperçu navigateur ----------
const demoStructure = (id: number): Structure => {
  const bpm = 124 + (id % 5);
  const bar = 240_000 / bpm;
  const plan: [Section["kind"], number][] = [["intro", 16], ["build", 8], ["drop", 32], ["break", 16], ["drop", 32], ["outro", 16]];
  let b = 0;
  const sections = plan.map(([kind, bars]) => {
    const s = { kind, startMs: 120 + b * bar, endMs: 120 + (b + bars) * bar, startBar: b, bars };
    b += bars;
    return s;
  });
  return {
    bpm,
    firstBeatMs: 120,
    bars: b,
    sections,
    cues: [
      { hot: 0, name: "start", ms: 120 },
      { hot: 1, name: "drop", ms: sections[2].startMs },
      { hot: 2, name: "break", ms: sections[3].startMs },
      { hot: 3, name: "outro", ms: sections[5].startMs },
    ],
    leadSilenceMs: 110,
    tailSilenceMs: 2400,
    durationMs: 120 + b * bar + 2400,
    timbre: [],
    confidence: id % 3 === 0 ? 0.1 : 0.7,
  };
};

export async function structureRun(ids: number[] | null): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("structure_run", { ids });
}

export async function structureGet(id: number): Promise<Structure | null> {
  if (!inTauri) return id % 4 === 3 ? null : demoStructure(id);
  return invoke<Structure | null>("structure_get", { id });
}

export async function similarTracks(id: number, limit = 30): Promise<Similar[]> {
  if (!inTauri)
    return Array.from({ length: 12 }, (_, i) => ({ id: ((id + i * 7) % 40) + 1, score: 92 - i * 4, keyCompat: ["same", "adjacent", "relative", "clash"][i % 4], bpmDiff: (i % 5) - 2, sound: 80 - i * 3 }));
  return invoke<Similar[]>("similar_tracks", { id, limit });
}

export async function setGenerate(name: string, plan: SetPlan): Promise<[number, number]> {
  if (!inTauri) return [1, Math.round(plan.minutes / 5)];
  return invoke<[number, number]>("set_generate", { name, plan });
}

export async function transitionPlay(a: number, b: number, bars: number): Promise<TransitionPlan> {
  if (!inTauri) return { aFromMs: 300_000, mixAtMs: 315_000, mixMs: 31_000, bFromMs: 120, bSpeed: 1.016, totalMs: 76_000, bars };
  return invoke<TransitionPlan>("transition_play", { a, b, bars });
}

export async function djeditExport(id: number, plan: EditPlan): Promise<EditResult> {
  if (!inTauri) return { trackId: id, fileName: "Morceau (DJ Edit).wav", durationMs: 420_000 };
  return invoke<EditResult>("djedit_export", { id, plan });
}

let demoInstalled = false;
export async function stemsStatus(): Promise<StemsStatus> {
  if (!inTauri) return { supported: true, installed: demoInstalled, downloadBytes: demoInstalled ? 0 : 221_884_896 };
  return invoke<StemsStatus>("stems_status");
}

export async function stemsInstall(): Promise<boolean> {
  if (!inTauri) {
    demoInstalled = true;
    return true;
  }
  return invoke<boolean>("stems_install");
}

export async function stemsSeparate(id: number, kind: StemsKind): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("stems_separate", { id, kind });
}

export async function vocalsDetect(ids: number[], yesTag: string, noTag: string): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("vocals_detect", { ids, yesTag, noTag });
}

export async function usbCheck(path: string, profile: UsbProfile): Promise<UsbReport> {
  if (!inTauri)
    return {
      filesystem: "exFAT",
      filesystemOk: profile === "cdj3000",
      files: 248,
      ok: 243,
      issues: [
        { path: "Sets/Mariage/Artiste - Titre.flac", kind: "format", detail: "FLAC" },
        { path: "Sets/Club/Remix (32 bits).wav", kind: "float", detail: "32 bits flottant" },
        { path: "Sets/Club/Abîmé.mp3", kind: "unreadable", detail: "fin de fichier inattendue" },
      ],
      rekordboxExport: true,
      bytes: 3_200_000_000,
    };
  return invoke<UsbReport>("usb_check", { path, profile });
}

/** Couleur d'une section (forme d'onde, détail). */
export const SECTION_COLORS: Record<Section["kind"], string> = {
  intro: "var(--section-intro)",
  build: "var(--section-build)",
  drop: "var(--section-drop)",
  break: "var(--section-break)",
  outro: "var(--section-outro)",
  main: "var(--section-intro)",
};
