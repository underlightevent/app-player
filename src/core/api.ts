/**
 * Seul point de contact entre l'interface et le cœur Rust.
 * Aucune logique métier ici : on transmet les demandes et les réponses.
 *
 * Hors de Tauri (aperçu dans un navigateur pendant le développement),
 * un faux cœur minimal garde les réglages en mémoire.
 */
import { invoke } from "@tauri-apps/api/core";

export type Settings = Record<string, string>;

export interface CoreError {
  code: string;
  detail: string;
}

export interface StartupStatus {
  version: string;
  platform: string;
  database_ok: boolean;
  database_error: CoreError | null;
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Aperçu : « ?demo » saute le premier lancement, « ?theme=dark » et « ?lang=en » choisissent l'apparence
// (servent aux captures d'écran du guide).
const previewParams = typeof location !== "undefined" ? new URLSearchParams(location.search) : new URLSearchParams();
const previewSettings: Settings = {
  theme: previewParams.get("theme") ?? "light",
  language: previewParams.get("lang") ?? "fr",
  onboarding_done: previewParams.has("demo") ? "true" : "false",
};

export function isCoreError(e: unknown): e is CoreError {
  return typeof e === "object" && e !== null && "code" in e && "detail" in e;
}

export async function startupStatus(): Promise<StartupStatus> {
  if (!inTauri) {
    return { version: "aperçu", platform: "preview", database_ok: true, database_error: null };
  }
  return invoke<StartupStatus>("startup_status");
}

export async function getSettings(): Promise<Settings> {
  if (!inTauri) return { ...previewSettings };
  return invoke<Settings>("get_settings");
}

export async function setSetting(key: string, value: string): Promise<void> {
  if (!inTauri) {
    previewSettings[key] = value;
    return;
  }
  await invoke("set_setting", { key, value });
}

export async function diagnosticReport(): Promise<string> {
  if (!inTauri) return "Aperçu navigateur : pas de rapport.";
  return invoke<string>("diagnostic_report");
}

export function logMessage(level: "info" | "warn" | "error", message: string) {
  if (!inTauri) {
    console[level](message);
    return;
  }
  invoke("log_message", { level, message }).catch(() => {
    /* le journal ne doit jamais faire planter l'interface */
  });
}
