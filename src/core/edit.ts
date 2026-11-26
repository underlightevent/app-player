/** Modification des tags, historique (annuler / rétablir) et pochettes. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

export interface CoverData {
  mime: string;
  /** base64 */
  data: string;
}

/** Champs à modifier. Absent : inchangé ; null : vidé. */
export interface Changes {
  title?: string | null;
  version?: string | null;
  artists?: string[];
  album?: string | null;
  year?: number | null;
  genre?: string | null;
  bpm?: number | null;
  key?: string | null;
  rating?: number;
  cover?: CoverData | null;
}

export interface EditFailure {
  trackId: number;
  fileName: string;
  code: string;
  message: string;
}

export interface EditReport {
  batchId: number | null;
  ok: number;
  failures: EditFailure[];
}

export interface HistoryResult {
  report: EditReport;
  label: string | null;
}

export interface PreparedCover {
  cover: CoverData;
  width: number;
  height: number;
  notSquare: boolean;
  preview: string;
}

export const EDITABLE_FIELDS = ["title", "version", "artists", "album", "year", "genre", "bpm", "key", "rating", "cover"] as const;
export type EditableField = (typeof EDITABLE_FIELDS)[number];

export async function tracksEdit(ids: number[], changes: Changes): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("tracks_edit", { ids, changes });
}

export async function tracksEditEach(items: [number, Changes][], label: string): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: items.length, failures: [] };
  return invoke<EditReport>("tracks_edit_each", { items, label });
}

export async function tracksSetLock(ids: number[], field: EditableField, locked: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke("tracks_set_lock", { ids, field, locked });
}

export async function historyApply(redo: boolean, batch?: number): Promise<HistoryResult> {
  if (!inTauri) return { report: { batchId: null, ok: 0, failures: [] }, label: null };
  return invoke<HistoryResult>("history_apply", { batch: batch ?? null, redo });
}

export async function trackCover(id: number, size = 600): Promise<string | null> {
  if (!inTauri) return null;
  return invoke<string | null>("track_cover", { id, size });
}

export async function coverPrepare(source: { path?: string; data?: string }): Promise<PreparedCover> {
  return invoke<PreparedCover>("cover_prepare", { path: source.path ?? null, data: source.data ?? null });
}

export async function libraryGenres(): Promise<string[]> {
  if (!inTauri) return ["Techno", "House", "Trance"];
  return invoke<string[]>("library_genres");
}

export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "gif", "bmp"];

export function isImagePath(p: string): boolean {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  return IMAGE_EXTENSIONS.includes(ext);
}

/** Choix d'une image dans la fenêtre native du système. */
export async function pickImage(title: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: false, title, filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }] });
  return typeof r === "string" ? r : null;
}

/** Image du presse-papiers (base64), si le presse-papiers contient une image. */
export async function clipboardImage(): Promise<string | null> {
  try {
    const items = await navigator.clipboard.read();
    for (const item of items) {
      const type = item.types.find((t) => t.startsWith("image/"));
      if (!type) continue;
      const blob = await item.getType(type);
      const buf = new Uint8Array(await blob.arrayBuffer());
      let bin = "";
      for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      return btoa(bin);
    }
  } catch {
    /* accès refusé ou pas d'image */
  }
  return null;
}
