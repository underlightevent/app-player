/** Rekordbox, playlists et filtres intelligents, conversion WAV → AIFF, mises à jour (phase 4). */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";
import type { EditReport } from "./edit";

export interface RekordboxStatus {
  file: string | null;
  importedAt: number | null;
  xmlTracks: number;
  matched: number;
  stale: boolean;
  reloadCount: number;
  journalCount: number;
  protected: boolean;
}

export interface ImportReport {
  xmlTracks: number;
  matched: number;
  unmatchedExamples: string[];
}

export interface Rule {
  field: string;
  op: string;
  value: string;
}

export interface Rules {
  matchAll: boolean;
  rules: Rule[];
}

export interface Playlist {
  id: number;
  name: string;
  kind: "static" | "smart";
  rules: Rules | null;
  count: number;
}

export interface ConvertItem {
  trackId: number;
  fileName: string;
  targetName: string;
  status: "ok" | "not_wav" | "unsupported" | "exists" | "offline";
  detail: string | null;
  inRekordbox: boolean;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  newer: boolean;
  url: string;
  notes: string | null;
}

/** Champs des règles des filtres intelligents, avec leur type. */
export const RULE_FIELDS: Record<string, "text" | "number" | "bool"> = {
  genre: "text",
  artist: "text",
  title: "text",
  version: "text",
  album: "text",
  label: "text",
  key: "text",
  format: "text",
  fileName: "text",
  year: "number",
  bpm: "number",
  rating: "number",
  duration: "number",
  rekordbox: "bool",
  hasCover: "bool",
};

export const RULE_OPS: Record<"text" | "number" | "bool", string[]> = {
  text: ["contains", "not_contains", "is", "is_not", "empty", "not_empty"],
  number: ["gte", "lte", "eq", "empty"],
  bool: ["is_true", "is_false"],
};

// ---------- Aperçu navigateur ----------
const now = Math.floor(Date.now() / 1000);
let previewPlaylists: Playlist[] = [
  { id: 1, name: "Warm-up vendredi", kind: "static", rules: null, count: 24 },
  { id: 2, name: "Peak time", kind: "static", rules: null, count: 38 },
  {
    id: 3,
    name: "Techno 4 étoiles et plus",
    kind: "smart",
    rules: { matchAll: true, rules: [{ field: "genre", op: "contains", value: "Techno" }, { field: "rating", op: "gte", value: "4" }] },
    count: 167,
  },
  { id: 4, name: "Sans pochette dans Rekordbox", kind: "smart", rules: { matchAll: true, rules: [{ field: "rekordbox", op: "is_true", value: "" }, { field: "hasCover", op: "is_false", value: "" }] }, count: 12 },
];
const previewStatus: RekordboxStatus = {
  file: "C:\\Users\\DJ\\Documents\\rekordbox.xml",
  importedAt: now - 86400 * 2,
  xmlTracks: 1874,
  matched: 1802,
  stale: false,
  reloadCount: 7,
  journalCount: 31,
  protected: true,
};

// ---------- Fenêtres du système ----------
export async function pickSave(title: string, defaultName: string, ext: string, filterName: string): Promise<string | null> {
  if (!inTauri) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const p = await save({ title, defaultPath: defaultName, filters: [{ name: filterName, extensions: [ext] }] });
  return p ?? null;
}

export async function pickFile(title: string, ext: string, filterName: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ title, multiple: false, directory: false, filters: [{ name: filterName, extensions: [ext] }] });
  return typeof r === "string" ? r : null;
}

/** Ouvre une page du projet sur GitHub dans le navigateur. */
export async function openReleasePage(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, "_blank");
    return;
  }
  await invoke("open_release_page", { url });
}

// ---------- Rekordbox ----------
export async function rekordboxImport(path: string): Promise<ImportReport> {
  return invoke<ImportReport>("rekordbox_import", { path });
}

export async function rekordboxStatus(): Promise<RekordboxStatus> {
  if (!inTauri) return previewStatus;
  return invoke<RekordboxStatus>("rekordbox_status");
}

export async function rekordboxReloadClear(ids: number[] | null): Promise<void> {
  if (!inTauri) return;
  await invoke("rekordbox_reload_clear", { ids });
}

/** Renvoie [morceaux exportés, morceaux ignorés (disque débranché)]. */
export async function exportM3u(ids: number[] | null, path: string): Promise<[number, number]> {
  return invoke<[number, number]>("export_m3u", { ids, path });
}

export async function rekordboxJournalExport(path: string, onlyRekordbox: boolean): Promise<void> {
  await invoke("rekordbox_journal_export", { path, onlyRekordbox });
}

export async function rekordboxExportXml(ids: number[] | null, playlistIds: number[], path: string): Promise<[number, number]> {
  return invoke<[number, number]>("rekordbox_export_xml", { ids, playlistIds, path });
}

// ---------- Playlists ----------
export async function playlistsList(): Promise<Playlist[]> {
  if (!inTauri) return previewPlaylists;
  return invoke<Playlist[]>("playlists_list");
}

export async function playlistCreate(name: string, rules: Rules | null, ids: number[] | null = null): Promise<number> {
  if (!inTauri) {
    const id = Math.max(0, ...previewPlaylists.map((p) => p.id)) + 1;
    previewPlaylists = [...previewPlaylists, { id, name, kind: rules ? "smart" : "static", rules, count: ids?.length ?? 0 }];
    return id;
  }
  return invoke<number>("playlist_create", { name, rules, ids });
}

export async function playlistUpdate(id: number, name: string | null, rules: Rules | null): Promise<void> {
  if (!inTauri) {
    previewPlaylists = previewPlaylists.map((p) => (p.id === id ? { ...p, name: name ?? p.name, rules: rules ?? p.rules } : p));
    return;
  }
  await invoke("playlist_update", { id, name, rules });
}

export async function playlistDelete(id: number): Promise<void> {
  if (!inTauri) {
    previewPlaylists = previewPlaylists.filter((p) => p.id !== id);
    return;
  }
  await invoke("playlist_delete", { id });
}

export async function playlistAdd(id: number, ids: number[]): Promise<number> {
  if (!inTauri) return ids.length;
  return invoke<number>("playlist_add", { id, ids });
}

export async function playlistRemove(id: number, ids: number[]): Promise<void> {
  if (!inTauri) return;
  await invoke("playlist_remove", { id, ids });
}

export async function playlistTrackIds(id: number): Promise<number[]> {
  if (!inTauri) return [1, 2, 3];
  return invoke<number[]>("playlist_track_ids", { id });
}

// ---------- Conversion WAV → AIFF ----------
export async function convertPreview(ids: number[]): Promise<ConvertItem[]> {
  if (!inTauri)
    return ids.slice(0, 3).map((id, i) => ({
      trackId: id,
      fileName: `Artiste ${id} - Morceau ${id}.wav`,
      targetName: `Artiste ${id} - Morceau ${id}.aiff`,
      status: "ok",
      detail: null,
      inRekordbox: i === 0,
    }));
  return invoke<ConvertItem[]>("convert_preview", { ids });
}

export async function convertApply(ids: number[]): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("convert_apply", { ids });
}

// ---------- Mises à jour ----------
export async function updateCheck(): Promise<UpdateInfo> {
  if (!inTauri) return { current: "0.5.0", latest: "0.5.0", newer: false, url: "", notes: null };
  return invoke<UpdateInfo>("update_check");
}
