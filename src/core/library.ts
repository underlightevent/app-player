/**
 * Appels du cœur pour la bibliothèque, les disques et les tâches de fond.
 * Hors de Tauri (aperçu navigateur), un faux jeu de données permet de voir l'interface.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { inTauri } from "./api";

export interface SortKey {
  field: string;
  desc: boolean;
}

export interface LibraryQuery {
  search: string;
  formats: string[];
  noCover: boolean;
  minRating: number;
  status: string | null;
  rootId: number | null;
  /** Dossier : [disque, chemin relatif] — ses morceaux et ceux de ses sous-dossiers. */
  folder: [number, string] | null;
  /** Compteur du tableau de bord « Santé de la bibliothèque ». */
  health: string | null;
  /** Playlist ou filtre intelligent. */
  playlist: number | null;
  /** « in » : dans Rekordbox ; « reload » : à recharger dans Rekordbox. */
  rekordbox: string | null;
  /** Étiquette personnelle (phase 6). */
  tag: number | null;
  /** Historique de jeu : « never » ou « recent » (30 jours). */
  played: string | null;
  sort: SortKey[];
}

export const emptyQuery: LibraryQuery = {
  search: "",
  formats: [],
  noCover: false,
  minRating: 0,
  status: null,
  rootId: null,
  folder: null,
  health: null,
  playlist: null,
  rekordbox: null,
  tag: null,
  played: null,
  sort: [],
};

export interface TrackRow {
  id: number;
  title: string | null;
  version: string | null;
  artists: string[];
  mainArtist: string | null;
  album: string | null;
  year: number | null;
  genre: string | null;
  bpm: number | null;
  key: string | null;
  rating: number;
  format: string;
  codec: string | null;
  bitrate: number | null;
  sampleRate: number | null;
  bitDepth: number | null;
  channels: number | null;
  durationMs: number | null;
  size: number;
  fileName: string;
  relPath: string;
  volumeId: number;
  volumeLabel: string;
  status: "ok" | "missing" | "unreadable" | "quarantined";
  error: string | null;
  online: boolean;
  path: string | null;
  coverCount: number;
  coverWidth: number | null;
  wavDesync: boolean;
  locked: string[];
  /** Présent dans la collection Rekordbox importée. */
  inRekordbox: boolean;
  /** Tags modifiés depuis : à recharger dans Rekordbox. */
  reload: boolean;
  /** Phase 6 */
  userTags: { id: number; name: string; color: string }[];
  playCount: number;
  lastPlayed: string | null;
  lufs: number | null;
  energy: number | null;
}

export interface RootInfo {
  id: number;
  volumeId: number;
  volumeLabel: string;
  relPath: string;
  path: string | null;
  online: boolean;
  kind: string;
  trackCount: number;
}

export interface VolumeStatus {
  id: number;
  label: string;
  online: boolean;
  mount: string | null;
}

export interface TaskInfo {
  id: number;
  label: string;
  detail: string | null;
  done: number;
  total: number;
  state: "running" | "done" | "cancelled" | "error";
  error: string | null;
  started_ms: number;
}

// ---------- Aperçu navigateur : données fictives (?rows=100000 pour le test de défilement) ----------
const previewCount = typeof location !== "undefined" ? Number(new URLSearchParams(location.search).get("rows")) || 2000 : 2000;
const previewRows: TrackRow[] = Array.from({ length: previewCount }, (_, i) => ({
  id: i + 1,
  title: `Morceau ${i + 1}`,
  version: i % 3 === 0 ? "Extended Mix" : i % 3 === 1 ? "Original Mix" : null,
  artists: [`Artiste ${i % 40}`, ...(i % 5 === 0 ? ["Invité"] : [])],
  mainArtist: `Artiste ${i % 40}`,
  album: i % 2 ? `Album ${i % 25}` : null,
  year: 2015 + (i % 10),
  genre: ["Techno", "House", "Trance", "Drum & Bass"][i % 4],
  bpm: 120 + (i % 30),
  key: ["Am", "C", "F#m", "Eb"][i % 4],
  rating: i % 6,
  format: ["wav", "mp3", "flac", "aiff"][i % 4],
  codec: null,
  bitrate: i % 4 === 1 ? 320000 : 1411000,
  sampleRate: 44100,
  bitDepth: 16,
  channels: 2,
  durationMs: 180000 + i * 1000,
  size: 50_000_000,
  fileName: `Artiste ${i % 40} - Morceau ${i + 1}.wav`,
  relPath: `Musique/Morceau ${i + 1}.wav`,
  volumeId: 1,
  volumeLabel: "MUSIQUE",
  status: i === 7 ? "missing" : "ok",
  error: null,
  online: i % 50 !== 3,
  path: null,
  coverCount: i % 7 === 0 ? 0 : 1,
  coverWidth: 1000,
  wavDesync: false,
  locked: [],
  inRekordbox: i % 3 !== 2,
  reload: i % 40 === 0,
  userTags: i % 9 === 0 ? [{ id: 1, name: "Mariage", color: "pink" }] : i % 13 === 0 ? [{ id: 3, name: "Dancefloor", color: "orange" }] : [],
  playCount: i % 5,
  lastPlayed: i % 5 ? "2026-11-" + String(1 + (i % 27)).padStart(2, "0") : null,
  lufs: -6 - (i % 9),
  energy: 1 + (i % 10),
}));

export async function libraryQuery(query: LibraryQuery): Promise<number[]> {
  if (!inTauri) {
    const s = query.search.toLowerCase();
    let rows = previewRows.filter(
      (r) =>
        (!s || `${r.title} ${r.mainArtist} ${r.genre}`.toLowerCase().includes(s)) &&
        (query.formats.length === 0 || query.formats.includes(r.format)) &&
        (!query.noCover || r.coverCount === 0) &&
        r.rating >= query.minRating,
    );
    const k = query.sort[0];
    if (k) {
      const get = (r: TrackRow) => (r as unknown as Record<string, unknown>)[k.field] as string | number | null;
      rows = [...rows].sort((a, b) => {
        const x = get(a) ?? "";
        const y = get(b) ?? "";
        return (x < y ? -1 : x > y ? 1 : 0) * (k.desc ? -1 : 1);
      });
    }
    return rows.map((r) => r.id);
  }
  return invoke<number[]>("library_query", { query });
}

export async function libraryRows(ids: number[]): Promise<TrackRow[]> {
  if (!inTauri) return ids.map((id) => previewRows[id - 1]).filter(Boolean);
  return invoke<TrackRow[]>("library_rows", { ids });
}

export async function libraryRoots(): Promise<RootInfo[]> {
  if (!inTauri)
    return [
      { id: 1, volumeId: 1, volumeLabel: "MUSIQUE", relPath: "Musique", path: "E:\\Musique", online: true, kind: "library", trackCount: 2000 },
    ];
  return invoke<RootInfo[]>("library_roots");
}

export async function libraryVolumes(): Promise<VolumeStatus[]> {
  if (!inTauri) return [{ id: 1, label: "MUSIQUE", online: true, mount: "E:\\" }];
  return invoke<VolumeStatus[]>("library_volumes");
}

export async function libraryAddRoot(path: string, kind = "library"): Promise<number> {
  return invoke<number>("library_add_root", { path, kind });
}

export async function libraryRemoveRoot(id: number): Promise<void> {
  await invoke("library_remove_root", { id });
}

export async function libraryRescan(): Promise<void> {
  if (!inTauri) return;
  await invoke("library_rescan");
}

export async function tasksList(): Promise<TaskInfo[]> {
  if (!inTauri) return [];
  return invoke<TaskInfo[]>("tasks_list");
}

export async function taskCancel(id: number): Promise<void> {
  await invoke("task_cancel", { id });
}

/** Choix d'un dossier dans la fenêtre native du système. */
export async function pickFolder(title: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title });
  return typeof r === "string" ? r : null;
}

export function onEvent<T>(name: string, handler: (payload: T) => void): () => void {
  if (!inTauri) return () => {};
  let un: UnlistenFn | null = null;
  let cancelled = false;
  void listen<T>(name, (e) => handler(e.payload)).then((f) => {
    if (cancelled) f();
    else un = f;
  });
  return () => {
    cancelled = true;
    un?.();
  };
}
