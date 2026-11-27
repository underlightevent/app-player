/** Veille musicale (phase 5) : classements, nouvelles sorties, liste d'envies, compte Spotify. */
import { invoke } from "@tauri-apps/api/core";
import { inTauri } from "./api";

export interface WatchItem {
  rank: number;
  title: string;
  artists: string[];
  version: string | null;
  label: string | null;
  releaseDate: string | null;
  isrc: string | null;
  coverUrl: string | null;
  source: "apple" | "deezer" | "musicbrainz" | string;
  sourceUrl: string | null;
  kind: "track" | "album" | string;
  inLibrary: boolean;
  inWishlist: boolean;
}

export interface WishItem extends WatchItem {
  id: number;
  status: "wanted" | "found" | "ignored";
  trackId: number | null;
  addedAt: number;
}

export interface Genre {
  id: number;
  name: string;
}

export interface SpotifyStatus {
  clientId: string;
  /** Nom du compte lié ; vide si aucun compte n'est lié. */
  user: string;
  redirect: string;
}

export interface SpotifyPlaylist {
  id: string;
  name: string;
  tracks: number;
}

export interface SpotifyMatch {
  wishId: number;
  uri: string | null;
  name: string;
  artists: string[];
  album: string;
  image: string | null;
  url: string | null;
}

/** Pays proposés pour les classements Apple Music (mêmes codes que le cœur). */
export const COUNTRIES = [
  "fr", "be", "ch", "ca", "lu", "us", "gb", "de", "es", "it", "nl", "pt", "ie", "at", "se", "no", "dk", "pl", "br", "mx", "ar",
  "co", "au", "jp", "kr", "za", "ma", "dz", "tn", "sn", "ci", "ng", "ae", "tr", "in",
];

/** Liens de recherche vers les boutiques (rien n'est acheté par l'application). */
export function shopLinks(i: Pick<WatchItem, "title" | "artists">): { shop: string; url: string }[] {
  const q = encodeURIComponent(`${i.artists[0] ?? ""} ${i.title}`.trim());
  return [
    { shop: "Beatport", url: `https://www.beatport.com/search?q=${q}` },
    { shop: "Bandcamp", url: `https://bandcamp.com/search?q=${q}` },
    { shop: "Juno", url: `https://www.junodownload.com/search/?q%5Ball%5D%5B%5D=${q}` },
    { shop: "Traxsource", url: `https://www.traxsource.com/search?term=${q}` },
  ];
}

// ---------- Données de l'aperçu navigateur ----------

const demo = (n: number, source: string, kind = "track"): WatchItem[] =>
  Array.from({ length: n }, (_, i) => ({
    rank: source === "musicbrainz" ? 0 : i + 1,
    title: ["Strobe", "Paris", "Dai Dai", "Losing It", "Turn Me On", "Silence", "Cola", "Sweet Dreams"][i % 8] + (i >= 8 ? ` ${i}` : ""),
    artists: [["deadmau5"], ["Kungs"], ["Shakira"], ["FISHER"], ["Kevin Lyttle"], ["Delerium", "Sarah McLachlan"], ["CamelPhat", "Elderbrook"], ["Eurythmics"]][i % 8],
    version: i % 3 === 0 ? "Extended Mix" : null,
    label: source === "musicbrainz" ? "Spinnin' Records" : null,
    releaseDate: `2026-11-${String(27 - (i % 20)).padStart(2, "0")}`,
    isrc: null,
    coverUrl: null,
    source,
    sourceUrl: null,
    kind,
    inLibrary: i % 4 === 1,
    inWishlist: i % 5 === 2,
  }));

let demoWish: WishItem[] = demo(6, "apple").map((x, i) => ({
  ...x,
  id: i + 1,
  status: i === 1 ? "found" : i === 4 ? "ignored" : "wanted",
  trackId: i === 1 ? 1 : null,
  addedAt: 1795700000 - i * 3600,
  inWishlist: true,
}));

// ---------- Appels au cœur ----------

export async function watchChart(source: "apple" | "deezer", country: string, genre: number, kind: "songs" | "albums", limit: number): Promise<WatchItem[]> {
  if (!inTauri) return demo(limit > 25 ? 25 : limit, source, kind === "albums" ? "album" : "track");
  return invoke<WatchItem[]>("watch_chart", { source, country, genre, kind, limit });
}

export async function watchGenres(): Promise<Genre[]> {
  if (!inTauri) return [{ id: 0, name: "Tous" }, { id: 113, name: "Dance" }, { id: 106, name: "Electro" }, { id: 116, name: "Rap/Hip Hop" }];
  return invoke<Genre[]>("watch_genres");
}

export async function watchReleases(label: string | null, country: string | null, days: number): Promise<WatchItem[]> {
  if (!inTauri) return demo(12, "musicbrainz");
  return invoke<WatchItem[]>("watch_releases", { label, country, days });
}

export async function wishlistList(): Promise<WishItem[]> {
  if (!inTauri) return demoWish;
  return invoke<WishItem[]>("wishlist_list");
}

export async function wishlistCount(): Promise<number> {
  if (!inTauri) return demoWish.filter((w) => w.status === "wanted").length;
  return invoke<number>("wishlist_count");
}

export async function wishlistAdd(items: WatchItem[]): Promise<number> {
  if (!inTauri) return items.length;
  return invoke<number>("wishlist_add", { items });
}

export async function wishlistSetIgnored(ids: number[], ignored: boolean): Promise<void> {
  if (!inTauri) {
    demoWish = demoWish.map((w) => (ids.includes(w.id) ? { ...w, status: ignored ? "ignored" : "wanted" } : w));
    return;
  }
  await invoke("wishlist_set_ignored", { ids, ignored });
}

export async function wishlistRemove(ids: number[], removed: boolean): Promise<void> {
  if (!inTauri) return;
  await invoke("wishlist_remove", { ids, removed });
}

export async function wishlistExportCsv(path: string): Promise<void> {
  await invoke("wishlist_export_csv", { path });
}

export async function watchOpenUrl(url: string): Promise<void> {
  if (!inTauri) {
    window.open(url, "_blank", "noopener");
    return;
  }
  await invoke("watch_open_url", { url });
}

export async function spotifyStatus(): Promise<SpotifyStatus> {
  if (!inTauri) return { clientId: "", user: "", redirect: "http://127.0.0.1:43821/callback" };
  return invoke<SpotifyStatus>("spotify_status");
}

export async function spotifyConnect(): Promise<string> {
  return invoke<string>("spotify_connect");
}

export async function spotifyDisconnect(): Promise<void> {
  await invoke("spotify_disconnect");
}

export async function spotifyPlaylists(): Promise<SpotifyPlaylist[]> {
  return invoke<SpotifyPlaylist[]>("spotify_playlists");
}

export async function spotifyCreatePlaylist(name: string): Promise<SpotifyPlaylist> {
  return invoke<SpotifyPlaylist>("spotify_create_playlist", { name });
}

export async function spotifyFind(ids: number[]): Promise<SpotifyMatch[]> {
  return invoke<SpotifyMatch[]>("spotify_find", { ids });
}

export async function spotifyAdd(playlistId: string, uris: string[]): Promise<number> {
  return invoke<number>("spotify_add", { playlistId, uris });
}
