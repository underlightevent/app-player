/** Outils du DJ (phase 6) : étiquettes, statistiques, sets, historique, sauvegarde, copie USB,
 * volume perçu, analyse BPM / tonalité / énergie, réparation des dates. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";
import type { EditReport } from "./edit";

export interface UserTag {
  id: number;
  name: string;
  color: string;
  count: number;
}

export const TAG_COLORS = ["gray", "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"] as const;

export interface Count {
  label: string;
  count: number;
  size: number;
}

export interface Stats {
  tracks: number;
  size: number;
  durationMs: number;
  genres: Count[];
  years: Count[];
  formats: Count[];
  quality: Count[];
  disks: Count[];
  addedByMonth: Count[];
  mostPlayed: Count[];
  keys: Count[];
  bpmRanges: Count[];
}

export interface SetTrack {
  id: number;
  title: string | null;
  version: string | null;
  artist: string | null;
  bpm: number | null;
  key: string | null;
  camelot: string | null;
  genre: string | null;
  durationMs: number | null;
  energy: number | null;
  /** Avec le morceau précédent : same, adjacent, relative, energy, clash, unknown. */
  keyCompat: string | null;
  bpmDiff: number | null;
}

export interface Suggestion extends SetTrack {
  score: number;
}

export interface HistoryReport {
  sessions: number;
  plays: number;
  unmatched: number;
  unmatchedExamples: string[];
}

export interface CopyReport {
  copied: number;
  skipped: number;
  bytes: number;
  failures: { file: string; message: string }[];
  offline: number;
  cancelled: boolean;
}

export type CopyLayout = "flat" | "playlist" | "folders";

export interface Proposal {
  trackId: number;
  title: string;
  currentBpm: number | null;
  currentKey: string | null;
  bpm: number | null;
  key: string | null;
  energy: number | null;
}

export interface RepairItem {
  trackId: number;
  fileName: string;
  frames: string[];
}

// ---------- Aperçu navigateur ----------
let demoTags: UserTag[] = [
  { id: 1, name: "Mariage", color: "pink", count: 42 },
  { id: 2, name: "Cocktail", color: "teal", count: 18 },
  { id: 3, name: "Dancefloor", color: "orange", count: 120 },
  { id: 4, name: "Version clean", color: "green", count: 9 },
];
const c = (label: string, count: number, size = 0): Count => ({ label, count, size });
const demoSet: SetTrack[] = [
  ["Strobe", "deadmau5", 128, "Am", "8A", null, null],
  ["Paris", "Kungs", 124, "Em", "9A", "adjacent", -3.1],
  ["Losing It", "FISHER", 125, "Em", "9A", "same", 0.8],
  ["Cola", "CamelPhat", 122, "F#", "2B", "clash", -2.4],
].map(([title, artist, bpm, key, camelot, keyCompat, bpmDiff], i) => ({
  id: i + 1,
  title: title as string,
  version: i === 0 ? "Club Edit" : null,
  artist: artist as string,
  bpm: bpm as number,
  key: key as string,
  camelot: camelot as string,
  genre: "House",
  durationMs: 300000 + i * 17000,
  energy: 6 + (i % 3),
  keyCompat: keyCompat as string | null,
  bpmDiff: bpmDiff as number | null,
}));

// ---------- Étiquettes ----------
export async function tagsList(): Promise<UserTag[]> {
  if (!inTauri) return demoTags;
  return invoke<UserTag[]>("tags_list");
}
export async function tagCreate(name: string, color: string): Promise<number> {
  if (!inTauri) {
    const id = Math.max(0, ...demoTags.map((t) => t.id)) + 1;
    demoTags = [...demoTags, { id, name, color, count: 0 }];
    return id;
  }
  return invoke<number>("tag_create", { name, color });
}
export async function tagUpdate(id: number, name: string, color: string): Promise<void> {
  if (!inTauri) {
    demoTags = demoTags.map((t) => (t.id === id ? { ...t, name, color } : t));
    return;
  }
  await invoke("tag_update", { id, name, color });
}
export async function tagDelete(id: number): Promise<void> {
  if (!inTauri) {
    demoTags = demoTags.filter((t) => t.id !== id);
    return;
  }
  await invoke("tag_delete", { id });
}
export async function tagApply(tag: number, ids: number[], on: boolean): Promise<number> {
  if (!inTauri) return ids.length;
  return invoke<number>("tag_apply", { tag, ids, on });
}

// ---------- Statistiques ----------
export async function statsGet(): Promise<Stats> {
  if (!inTauri)
    return {
      tracks: 3273,
      size: 41_200_000_000,
      durationMs: 3273 * 240_000,
      genres: [c("House", 820), c("Rap FR", 610), c("Dance", 400), c("Pop", 330), c("Moombahton", 210), c("Afro", 190), c("Techno", 160)],
      years: [c("1970", 40), c("1980", 120), c("1990", 260), c("2000", 520), c("2010", 1100), c("2020", 1233)],
      formats: [c("MP3", 2600, 26e9), c("WAV", 520, 13e9), c("FLAC", 120, 2e9), c("AIFF", 33, 0.2e9)],
      quality: [c("320", 2200), c("lossless", 673), c("256", 200), c("192", 150), c("low", 50)],
      disks: [c("MUSICS (E:)", 3273, 41.2e9)],
      addedByMonth: [c("2026-11", 340), c("2026-10", 120), c("2026-09", 210), c("2026-08", 90)],
      mostPlayed: [c("Kungs - Paris", 14), c("FISHER - Losing It", 11), c("deadmau5 - Strobe", 9)],
      keys: [c("1A", 120), c("2A", 160), c("3A", 140), c("4A", 210), c("5A", 230), c("6A", 180), c("8A", 320), c("8B", 90), c("9A", 260), c("10A", 200), c("11A", 170), c("12A", 150)],
      bpmRanges: [c("80", 120), c("90", 300), c("100", 260), c("110", 180), c("120", 1400), c("130", 520), c("140", 160), c("170", 90)],
    };
  return invoke<Stats>("stats_get");
}

// ---------- Sets ----------
export async function setCreate(name: string): Promise<number> {
  if (!inTauri) return 99;
  return invoke<number>("set_create", { name });
}
export async function setTracks(id: number): Promise<SetTrack[]> {
  if (!inTauri) return demoSet;
  return invoke<SetTrack[]>("set_tracks", { id });
}
export async function setMove(id: number, track: number, to: number): Promise<void> {
  if (!inTauri) return;
  await invoke("set_move", { id, track, to });
}
export async function setSuggestions(after: number, exclude: number[], bpmRange: number): Promise<Suggestion[]> {
  if (!inTauri)
    return [
      { ...demoSet[2], id: 11, title: "Turn Me On", artist: "Kevin Lyttle", keyCompat: "same", bpmDiff: 0.8, score: 78 },
      { ...demoSet[1], id: 12, title: "Silence", artist: "Delerium", keyCompat: "adjacent", bpmDiff: -1.6, score: 64 },
    ];
  return invoke<Suggestion[]>("set_suggestions", { after, exclude, bpmRange });
}

// ---------- Historique ----------
export async function historyImport(path: string): Promise<HistoryReport> {
  return invoke<HistoryReport>("history_import", { path });
}
export async function historyClear(): Promise<void> {
  await invoke("history_clear");
}

// ---------- Sauvegarde et copie USB ----------
export async function backupRun(dest: string): Promise<CopyReport> {
  return invoke<CopyReport>("backup_run", { dest });
}
export async function usbCopy(ids: number[], dest: string, layout: CopyLayout, name: string, m3u: boolean): Promise<CopyReport> {
  return invoke<CopyReport>("usb_copy", { ids, dest, layout, name, m3u });
}

// ---------- Volume, analyse, dates ----------
export async function loudnessRun(ids: number[] | null): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("loudness_run", { ids });
}
export async function analysisRun(ids: number[] | null): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("analysis_run", { ids });
}
export async function analysisProposals(): Promise<Proposal[]> {
  if (!inTauri)
    return [
      { trackId: 1, title: "Kungs - Paris", currentBpm: null, currentKey: null, bpm: 124, key: "Em", energy: 7 },
      { trackId: 2, title: "Delerium - Silence", currentBpm: 125, currentKey: null, bpm: null, key: "Am", energy: 6 },
    ];
  return invoke<Proposal[]>("analysis_proposals");
}
export async function analysisApply(ids: number[]): Promise<EditReport> {
  return invoke<EditReport>("analysis_apply", { ids });
}
export async function datesPlan(ids: number[] | null): Promise<RepairItem[]> {
  if (!inTauri) return [{ trackId: 1, fileName: "Soprano - Clown (Original).mp3", frames: ["TDEN"] }];
  return invoke<RepairItem[]>("dates_plan", { ids });
}
export async function datesApply(ids: number[]): Promise<EditReport> {
  return invoke<EditReport>("dates_apply", { ids });
}

/** Libellé court d'une compatibilité harmonique. */
export const COMPAT_ICON: Record<string, string> = {
  same: "=",
  adjacent: "±1",
  relative: "A↔B",
  energy: "+2",
  clash: "✕",
  unknown: "?",
};
