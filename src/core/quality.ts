/** Doublons, quarantaine, contrôle qualité et santé de la bibliothèque (phase 3). */
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { inTauri } from "./api";
import type { EditReport } from "./edit";
import { libraryRows, type TrackRow } from "./library";

export interface DuplicateMember {
  track: TrackRow;
  similarity: number | null;
  suspect: boolean;
  cutoffHz: number | null;
}

export interface DuplicateGroup {
  id: number;
  kind: "exact" | "similar";
  keeperId: number | null;
  keeperManual: boolean;
  suggestedId: number | null;
  members: DuplicateMember[];
}

export interface ResolveRequest {
  groupId: number;
  keeperId?: number | null;
  transfer?: boolean;
}

export interface Transfer {
  field: string;
  value: string;
  fromFile: string;
}

export interface ResolvePlan {
  groupId: number;
  keeperId: number;
  keeperFile: string;
  transfers: Transfer[];
  discard: [number, string][];
  blockers: string[];
  inRekordbox: string[];
}

export interface QuarantineItem {
  id: number;
  trackId: number | null;
  fileName: string;
  title: string | null;
  artists: string | null;
  origRel: string;
  quarRel: string;
  volumeId: number;
  volumeLabel: string;
  size: number;
  reason: string | null;
  addedAt: number;
  online: boolean;
  exists: boolean;
  path: string | null;
}

export interface QuarantineSummary {
  count: number;
  size: number;
  offline: number;
}

export interface QualityInfo {
  verdict: "ok" | "suspect" | "corrupt";
  cutoffHz: number | null;
  decodeErrors: number;
  truncated: boolean;
  detail: string | null;
  analyzedAt: number;
}

export interface HealthCounter {
  key: string;
  count: number;
}

export interface HealthReport {
  total: number;
  counters: HealthCounter[];
  notAnalyzed: number;
  quarantined: number;
}

export interface Relocation {
  trackId: number;
  oldRel: string;
  newPath: string;
  method: "name_size" | "hash";
}

// ---------- Aperçu navigateur : données fictives ----------
const now = Math.floor(Date.now() / 1000);
let previewQuarantine: QuarantineItem[] = [
  {
    id: 1,
    trackId: 9001,
    fileName: "Artiste 3 - Morceau 12.mp3",
    title: "Morceau 12",
    artists: "Artiste 3",
    origRel: "Musique/House/Artiste 3 - Morceau 12.mp3",
    quarRel: "Musique/_Quarantaine/House/Artiste 3 - Morceau 12.mp3",
    volumeId: 1,
    volumeLabel: "MUSIQUE",
    size: 9_800_000,
    reason: "duplicate",
    addedAt: now - 3600,
    online: true,
    exists: true,
    path: null,
  },
  {
    id: 2,
    trackId: 9002,
    fileName: "Artiste 8 - Morceau 40 (copie).wav",
    title: "Morceau 40",
    artists: "Artiste 8",
    origRel: "Musique/Techno/Artiste 8 - Morceau 40 (copie).wav",
    quarRel: "Musique/_Quarantaine/Techno/Artiste 8 - Morceau 40 (copie).wav",
    volumeId: 1,
    volumeLabel: "MUSIQUE",
    size: 61_200_000,
    reason: "manual",
    addedAt: now - 86400 * 2,
    online: true,
    exists: true,
    path: null,
  },
];

async function previewGroups(): Promise<DuplicateGroup[]> {
  const rows = await libraryRows([1, 2, 5, 6, 9, 10]);
  const fix = (r: TrackRow, format: string, bitrate: number | null, title: string): TrackRow => ({
    ...r,
    title,
    version: "Extended Mix",
    mainArtist: "Artiste 1",
    artists: ["Artiste 1"],
    format,
    bitrate,
    bitDepth: format === "mp3" ? null : 16,
    fileName: `Artiste 1 - ${title} (Extended Mix).${format}`,
    durationMs: 386_000,
    coverCount: format === "mp3" ? 1 : 0,
  });
  return [
    {
      id: 1,
      kind: "similar",
      keeperId: rows[0].id,
      keeperManual: false,
      suggestedId: rows[0].id,
      members: [
        { track: fix(rows[0], "wav", 1411000, "Lumière"), similarity: 0.97, suspect: false, cutoffHz: null },
        { track: fix(rows[1], "mp3", 320000, "Lumière"), similarity: 0.97, suspect: false, cutoffHz: 19800 },
      ],
    },
    {
      id: 2,
      kind: "exact",
      keeperId: rows[2].id,
      keeperManual: false,
      suggestedId: rows[2].id,
      members: [
        { track: fix(rows[2], "flac", 1000000, "Minuit"), similarity: 1, suspect: false, cutoffHz: null },
        { track: { ...fix(rows[3], "flac", 1000000, "Minuit"), fileName: "Minuit (1).flac" }, similarity: 1, suspect: false, cutoffHz: null },
      ],
    },
    {
      id: 3,
      kind: "similar",
      keeperId: rows[5].id,
      keeperManual: false,
      suggestedId: rows[5].id,
      members: [
        { track: fix(rows[4], "wav", 1411000, "Aurore"), similarity: 0.93, suspect: true, cutoffHz: 16000 },
        { track: fix(rows[5], "mp3", 320000, "Aurore"), similarity: 0.93, suspect: false, cutoffHz: 20000 },
      ],
    },
  ];
}

const previewHealth: HealthReport = {
  total: 2000,
  notAnalyzed: 0,
  quarantined: 2,
  counters: [
    ["no_cover", 286],
    ["small_cover", 41],
    ["no_year", 12],
    ["no_genre", 7],
    ["no_rating", 333],
    ["review", 18],
    ["unidentified", 9],
    ["duplicates", 6],
    ["suspect", 3],
    ["low_bitrate", 24],
    ["bad_names", 57],
    ["long_paths", 0],
    ["bad_chars", 2],
    ["wav_desync", 4],
    ["missing", 1],
    ["unreadable", 1],
    ["corrupt", 0],
  ].map(([key, count]) => ({ key: key as string, count: count as number })),
};

// ---------- Doublons ----------
export async function duplicatesFind(): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("duplicates_find");
}

export async function duplicatesList(): Promise<DuplicateGroup[]> {
  if (!inTauri) return previewGroups();
  return invoke<DuplicateGroup[]>("duplicates_list");
}

export async function duplicatesSetKeeper(group: number, track: number): Promise<void> {
  if (!inTauri) return;
  await invoke("duplicates_set_keeper", { group, track });
}

export async function duplicatesNotDuplicates(group: number): Promise<void> {
  if (!inTauri) return;
  await invoke("duplicates_not_duplicates", { group });
}

export async function duplicatesPlan(requests: ResolveRequest[]): Promise<ResolvePlan[]> {
  if (!inTauri) {
    const groups = await previewGroups();
    return requests.map((r) => {
      const g = groups.find((x) => x.id === r.groupId)!;
      const keeperId = r.keeperId ?? g.keeperId!;
      const keeper = g.members.find((m) => m.track.id === keeperId)!;
      const others = g.members.filter((m) => m.track.id !== keeperId);
      return {
        groupId: g.id,
        keeperId,
        keeperFile: keeper.track.fileName,
        transfers: keeper.track.coverCount === 0 && others.some((o) => o.track.coverCount > 0) && r.transfer !== false ? [{ field: "cover", value: "1000 px", fromFile: others[0].track.fileName }] : [],
        discard: others.map((o) => [o.track.id, o.track.fileName] as [number, string]),
        blockers: [],
        inRekordbox: others.filter((o) => o.track.inRekordbox).map((o) => o.track.fileName),
      };
    });
  }
  return invoke<ResolvePlan[]>("duplicates_plan", { requests });
}

export async function duplicatesResolve(requests: ResolveRequest[]): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: requests.length, failures: [] };
  return invoke<EditReport>("duplicates_resolve", { requests });
}

// ---------- Quarantaine ----------
export async function quarantineTracks(ids: number[]): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("quarantine_tracks", { ids });
}

export async function quarantineList(): Promise<QuarantineItem[]> {
  if (!inTauri) return previewQuarantine;
  return invoke<QuarantineItem[]>("quarantine_list");
}

export async function quarantineRestore(ids: number[]): Promise<EditReport> {
  if (!inTauri) {
    previewQuarantine = previewQuarantine.filter((q) => !ids.includes(q.id));
    return { batchId: null, ok: ids.length, failures: [] };
  }
  return invoke<EditReport>("quarantine_restore", { ids });
}

export async function quarantineSummary(ids: number[] | null): Promise<QuarantineSummary> {
  if (!inTauri) {
    const list = previewQuarantine.filter((q) => !ids || ids.includes(q.id));
    return { count: list.length, size: list.reduce((s, q) => s + q.size, 0), offline: 0 };
  }
  return invoke<QuarantineSummary>("quarantine_summary", { ids });
}

export async function quarantineEmpty(ids: number[] | null): Promise<EditReport> {
  if (!inTauri) {
    const n = previewQuarantine.filter((q) => !ids || ids.includes(q.id)).length;
    previewQuarantine = previewQuarantine.filter((q) => ids && !ids.includes(q.id));
    return { batchId: null, ok: n, failures: [] };
  }
  return invoke<EditReport>("quarantine_empty", { ids });
}

// ---------- Qualité et santé ----------
export async function qualityAnalyze(ids: number[] | null): Promise<boolean> {
  if (!inTauri) return true;
  return invoke<boolean>("quality_analyze", { ids });
}

export async function qualityInfo(id: number): Promise<QualityInfo | null> {
  if (!inTauri) return id % 9 === 4 ? { verdict: "suspect", cutoffHz: 16000, decodeErrors: 0, truncated: false, detail: "fake_lossless:16.0", analyzedAt: now } : { verdict: "ok", cutoffHz: null, decodeErrors: 0, truncated: false, detail: null, analyzedAt: now };
  return invoke<QualityInfo | null>("quality_info", { id });
}

export async function qualitySpectrogram(id: number): Promise<string | null> {
  if (!inTauri) return null;
  return invoke<string>("quality_spectrogram", { id });
}

export async function healthReport(): Promise<HealthReport> {
  if (!inTauri) return previewHealth;
  return invoke<HealthReport>("health_report");
}

export async function relocatePreview(dir: string): Promise<Relocation[]> {
  return invoke<Relocation[]>("relocate_preview", { dir });
}

export async function relocateApply(items: Relocation[]): Promise<number> {
  return invoke<number>("relocate_apply", { items });
}

/** Explication lisible d'une analyse (« coupure nette à 16,0 kHz… »), via les traductions. */
export function qualityText(q: QualityInfo, t: TFunction): string {
  const [code, a, b] = (q.detail ?? "").split(":");
  switch (code) {
    case "fake_lossless":
      return t("quality.detail.fakeLossless", { khz: a });
    case "upscaled":
      return t("quality.detail.upscaled", { khz: a, kbps: b });
    case "truncated":
      return t("quality.detail.truncated", { got: a, expected: b });
    case "errors":
      return t("quality.detail.errors", { count: Number(a) });
    case "open":
    case "empty":
      return t("quality.detail.unreadable");
    default:
      return q.verdict === "ok" ? t("quality.detail.ok") : "";
  }
}

export async function duplicatesCount(): Promise<number> {
  if (!inTauri) return 3;
  return invoke<number>("duplicates_count");
}

/** WAV : remet RIFF INFO d'accord avec l'ID3. */
export async function tracksSyncWav(ids: number[]): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("tracks_sync_wav", { ids });
}
