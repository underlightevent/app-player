/** Dossiers : arborescence, création, déplacement ; base : export et import. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";
import type { EditReport } from "./edit";
import type { PlanItem } from "./rename";

export interface DirInfo {
  volumeId: number;
  name: string;
  relPath: string;
  trackCount: number;
  hasChildren: boolean;
}

/** Glisser-déposer de morceaux depuis le tableau. */
export const TRACKS_MIME = "application/x-djlm-tracks";

export async function foldersList(volumeId: number, rel: string): Promise<DirInfo[]> {
  if (!inTauri) {
    return rel.split("/").length > 2
      ? []
      : ["House", "Techno", "Trance"].map((n) => ({ volumeId, name: n, relPath: `${rel}/${n}`, trackCount: 120, hasChildren: true }));
  }
  return invoke<DirInfo[]>("folders_list", { volumeId, rel });
}

export async function folderCreate(volumeId: number, parent: string, name: string): Promise<EditReport> {
  return invoke<EditReport>("folder_create", { volumeId, parent, name });
}

export async function folderMove(volumeId: number, from: string, to: string): Promise<EditReport> {
  return invoke<EditReport>("folder_move", { volumeId, from, to });
}

export async function tracksMovePreview(ids: number[], volumeId: number, target: string): Promise<PlanItem[]> {
  if (!inTauri) return ids.map((id) => ({ id, folder: "Musique", oldName: `piste_${id}.wav`, newName: `${target}/piste_${id}.wav`, status: "ok", suffixed: false, longPath: false, inRekordbox: id % 3 === 0 }));
  return invoke<PlanItem[]>("tracks_move_preview", { ids, volumeId, target });
}

export async function tracksMove(ids: number[], volumeId: number, target: string): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("tracks_move", { ids, volumeId, target });
}

export async function reveal(target: { id?: number; volumeId?: number; rel?: string }): Promise<void> {
  if (!inTauri) return;
  await invoke("reveal", { id: target.id ?? null, volumeId: target.volumeId ?? null, rel: target.rel ?? null });
}

export async function databaseExport(title: string): Promise<boolean> {
  if (!inTauri) return false;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title, defaultPath: `dj-library-manager-${new Date().toISOString().slice(0, 10)}.sqlite`, filters: [{ name: "SQLite", extensions: ["sqlite"] }] });
  if (!path) return false;
  await invoke("database_export", { path });
  return true;
}

export async function databaseImport(title: string): Promise<boolean> {
  if (!inTauri) return false;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ title, multiple: false, filters: [{ name: "SQLite", extensions: ["sqlite"] }] });
  if (typeof path !== "string") return false;
  await invoke("database_import", { path });
  return true;
}
