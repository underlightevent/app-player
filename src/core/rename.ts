/** Renommage par modèle, nettoyage et tags depuis le nom de fichier. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";
import type { EditReport } from "./edit";

export const DEFAULT_TEMPLATE = "{artiste_principal} - {titre}[ ({version})]";
export const TEMPLATE_VARIABLES = ["artiste_principal", "artistes", "titre", "version", "album", "annee", "genre", "bpm", "tonalite", "note"];
export const PARSE_PRESETS = ["{artistes} - {titre}", "{piste}. {artistes} - {titre}", "{piste} - {artistes} - {titre}", "{titre}"];

export type OnConflict = "suffix" | "skip";

export interface PlanItem {
  id: number;
  folder: string;
  oldName: string;
  newName: string | null;
  status: "ok" | "same" | "conflict" | "reserved" | "empty" | "offline" | "missing";
  suffixed: boolean;
  longPath: boolean;
  /** Le morceau est dans Rekordbox : il y apparaîtra comme manquant. */
  inRekordbox: boolean;
}

export interface TagValues {
  title: string | null;
  version: string | null;
  artists: string[];
  album: string | null;
  year: number | null;
  genre: string | null;
  bpm: number | null;
  key: string | null;
}

export interface TagPreview {
  id: number;
  fileName: string;
  before: TagValues;
  after: TagValues;
  fields: string[];
}

export interface CustomRule {
  find: string;
  replace: string;
  regex: boolean;
  fields: string[];
}

export interface CleanRules {
  junk: boolean;
  trackNumbers: boolean;
  underscores: boolean;
  featToArtists: boolean;
  extractVersion: boolean;
  versionCase: boolean;
  titleCase: boolean;
  custom: CustomRule[];
}

export const defaultCleanRules: CleanRules = {
  junk: true,
  trackNumbers: true,
  underscores: true,
  featToArtists: true,
  extractVersion: true,
  versionCase: true,
  titleCase: false,
  custom: [],
};

export interface SavedTemplate {
  name: string;
  template: string;
}

const none: EditReport = { batchId: null, ok: 0, failures: [] };

export async function renamePreview(ids: number[], template: string, onConflict: OnConflict): Promise<PlanItem[]> {
  if (!inTauri) {
    return ids.slice(0, 50).map((id) => ({
      id,
      folder: "Musique",
      oldName: `piste_${id}.wav`,
      newName: `Artiste ${id % 40} - Morceau ${id}.wav`,
      status: id % 17 === 0 ? "conflict" : "ok",
      suffixed: false,
      longPath: false,
      inRekordbox: id % 3 === 0,
    }));
  }
  return invoke<PlanItem[]>("rename_preview", { ids, template, onConflict });
}

export async function renameApply(ids: number[], template: string, onConflict: OnConflict): Promise<EditReport> {
  if (!inTauri) return { ...none, ok: ids.length };
  return invoke<EditReport>("rename_apply", { ids, template, onConflict });
}

/** Message d'erreur du modèle, ou null s'il est valide. */
export async function renameValidate(template: string): Promise<string | null> {
  if (!inTauri) return /\{[^}]*$/.test(template) ? "accolade non fermée" : null;
  return invoke<string | null>("rename_validate", { template });
}

export async function cleanPreview(ids: number[], rules: CleanRules): Promise<TagPreview[]> {
  if (!inTauri) return [];
  return invoke<TagPreview[]>("clean_preview", { ids, rules });
}

export async function cleanApply(ids: number[], rules: CleanRules): Promise<EditReport> {
  if (!inTauri) return none;
  return invoke<EditReport>("clean_apply", { ids, rules });
}

export async function parsePreview(ids: number[], pattern: string, overwrite: boolean): Promise<TagPreview[]> {
  if (!inTauri) return [];
  return invoke<TagPreview[]>("parse_preview", { ids, pattern, overwrite });
}

export async function parseApply(ids: number[], pattern: string, overwrite: boolean): Promise<EditReport> {
  if (!inTauri) return none;
  return invoke<EditReport>("parse_apply", { ids, pattern, overwrite });
}
