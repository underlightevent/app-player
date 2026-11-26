/** Recherche en ligne : file de recherche, résultats à valider, recherche manuelle, clés. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";
import type { CoverData, EditReport } from "./edit";
import type { TagValues } from "./rename";
export { onEvent as onEventOnline } from "./library";

export interface CoverCandidate {
  url: string;
  source: string;
  width: number | null;
  height: number | null;
}

export interface Candidate {
  source: string;
  id: string;
  title: string | null;
  version: string | null;
  artists: string[];
  album: string | null;
  year: number | null;
  genre: string | null;
  bpm: number | null;
  label: string | null;
  isrc: string | null;
  durationMs: number | null;
  releaseId: string | null;
  covers: CoverCandidate[];
  fingerprintScore: number | null;
  confirmedBy: string[];
  score: number;
  scoreDetail: string[];
}

export interface ResultRow {
  trackId: number;
  status: "pending" | "auto" | "review" | "unidentified" | "error" | "accepted" | "rejected";
  score: number;
  candidates: Candidate[];
  applied: string[];
  detail: string | null;
  current: TagValues & { rating: number };
  fileName: string;
  durationMs: number | null;
  coverCount: number;
  releaseGroup: string | null;
}

export interface OnlineStatus {
  queued: number;
  offline: boolean;
  busy: boolean;
  review: number;
  unidentified: number;
  inbox: number;
  hasAcoustidKey: boolean;
}

export interface CoverPreview {
  url: string;
  source: string;
  width: number;
  height: number;
  preview: string;
}

export const emptyStatus: OnlineStatus = { queued: 0, offline: false, busy: false, review: 0, unidentified: 0, inbox: 0, hasAcoustidKey: false };

/** Champs proposables, dans l'ordre d'affichage. */
export const ONLINE_FIELDS = ["title", "version", "artists", "album", "year", "genre", "bpm"] as const;
export type OnlineField = (typeof ONLINE_FIELDS)[number];

export async function onlineSearch(ids: number[]): Promise<void> {
  if (!inTauri) return;
  await invoke("online_search", { ids });
}

export async function onlineStatus(): Promise<OnlineStatus> {
  if (!inTauri) return emptyStatus;
  return invoke<OnlineStatus>("online_status");
}

const previewRow = (i: number): ResultRow => ({
  trackId: i + 1,
  status: "review",
  score: 72 + (i % 15),
  candidates: [
    {
      source: "musicbrainz",
      id: `rec-${i}`,
      title: `Morceau ${i + 1}`,
      version: i % 2 ? "Extended Mix" : "Original Mix",
      artists: [`Artiste ${i % 40}`, "Invité"],
      album: `Album ${i % 5}`,
      year: 2018 + (i % 6),
      genre: "Techno",
      bpm: null,
      label: "Label",
      isrc: null,
      durationMs: 400000,
      releaseId: null,
      covers: [],
      fingerprintScore: i % 3 ? 0.93 : null,
      confirmedBy: [],
      score: 72 + (i % 15),
      scoreDetail: ["empreinte 0,93", "titre identique", "durée +1 s"],
    },
  ],
  applied: [],
  detail: "empreinte 0,93 · titre identique · durée +1 s",
  current: { title: `Morceau ${i + 1}`, version: null, artists: [`Artiste ${i % 40}`], album: null, year: null, genre: null, bpm: null, key: null, rating: 0 },
  fileName: `piste_${i + 1}.wav`,
  durationMs: 401000,
  coverCount: 0,
  releaseGroup: `Artiste ${i % 40}|Album ${i % 5}`,
});

export async function onlineResults(status: string): Promise<ResultRow[]> {
  if (!inTauri) return Array.from({ length: 12 }, (_, i) => previewRow(i));
  return invoke<ResultRow[]>("online_results", { status });
}

export async function onlineAccept(trackId: number, candidate: Candidate, fields: string[], coverUrl: string | null, cover: CoverData | null): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: 1, failures: [] };
  return invoke<EditReport>("online_accept", { trackId, candidate, fields, coverUrl, cover });
}

export async function onlineReject(trackId: number): Promise<void> {
  if (!inTauri) return;
  await invoke("online_reject", { trackId });
}

export async function onlineManualSearch(trackId: number, artist: string, title: string): Promise<Candidate[]> {
  if (!inTauri) return [previewRow(trackId).candidates[0]];
  return invoke<Candidate[]>("online_manual_search", { trackId, artist, title });
}

export async function onlineCoverPreviews(candidates: Candidate[]): Promise<CoverPreview[]> {
  if (!inTauri) return [];
  return invoke<CoverPreview[]>("online_cover_previews", { candidates });
}

export async function onlineCoverFetch(url: string, source: string): Promise<string> {
  return invoke<string>("online_cover_fetch", { url, source });
}

export async function onlineSetKey(name: "acoustid_client" | "acoustid_user", value: string): Promise<void> {
  await invoke("online_set_key", { name, value });
}

export async function onlineKeyStatus(): Promise<{ client: boolean; user: boolean }> {
  if (!inTauri) return { client: false, user: false };
  return invoke("online_key_status");
}

export async function onlineTest(): Promise<{ musicbrainz: string; acoustid: string }> {
  if (!inTauri) return { musicbrainz: "ok", acoustid: "no_key" };
  return invoke("online_test");
}

export async function onlineCallStats(): Promise<[string, number, number][]> {
  if (!inTauri) return [];
  return invoke("online_call_stats");
}

export async function inboxList(): Promise<[number, string][]> {
  if (!inTauri) return [[1, "done"], [2, "review"], [3, "pending"], [4, "unidentified"]];
  return invoke("inbox_list");
}

export async function inboxFile(ids: number[], volumeId: number, target: string): Promise<EditReport> {
  if (!inTauri) return { batchId: null, ok: ids.length, failures: [] };
  return invoke<EditReport>("inbox_file", { ids, volumeId, target });
}
