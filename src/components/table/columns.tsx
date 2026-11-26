import type { ReactNode } from "react";
import type { TFunction } from "i18next";
import { AlertTriangle, Lock, Unplug } from "lucide-react";
import type { TrackRow } from "../../core/library";
import { formatBpm, formatDuration, formatKey, formatQuality, isLowBitrate } from "../../core/format";

export interface ColumnDef {
  id: string;
  /** Champ de tri côté cœur (absent : colonne non triable). */
  sortField?: string;
  width: number;
  minWidth?: number;
  align?: "left" | "right" | "center";
  defaultVisible: boolean;
  render: (row: TrackRow, ctx: RenderCtx) => ReactNode;
}

export interface RenderCtx {
  t: TFunction;
  locale: string;
  keyNotation: "camelot" | "musical";
}

/** Étoiles en caractères (bien plus léger que des icônes pour des milliers de lignes). */
export function Stars({ value }: { value: number }) {
  return (
    <span className="stars-text" aria-label={`${value}/5`}>
      <span className="on">{"★".repeat(value)}</span>
      <span className="off">{"★".repeat(5 - value)}</span>
    </span>
  );
}

function Badges({ row, ctx }: { row: TrackRow; ctx: RenderCtx }) {
  const { t } = ctx;
  const out: ReactNode[] = [];
  if (!row.online) out.push(<span key="off" className="badge" title={t("badges.offline", { disk: row.volumeLabel })}><Unplug size={12} aria-hidden />{t("badges.offlineShort")}</span>);
  if (row.status === "missing") out.push(<span key="m" className="badge badge-error" title={t("badges.missing")}>{t("badges.missingShort")}</span>);
  if (row.status === "unreadable") out.push(<span key="u" className="badge badge-error" title={row.error ?? t("badges.unreadable")}><AlertTriangle size={12} aria-hidden />{t("badges.unreadableShort")}</span>);
  if (row.locked.length > 0) out.push(<span key="l" className="badge" title={t("badges.locked", { fields: row.locked.map((f) => t(`fields.${f}`, { defaultValue: f })).join(", ") })}><Lock size={12} aria-hidden /></span>);
  if (row.format === "wav") out.push(<span key="w" className="badge badge-info" title={t("badges.wav")}>WAV</span>);
  if (row.wavDesync) out.push(<span key="d" className="badge badge-warning" title={t("badges.desync")}>{t("badges.desyncShort")}</span>);
  return <span className="badges">{out}</span>;
}

export const COLUMNS: ColumnDef[] = [
  {
    id: "cover",
    width: 40,
    minWidth: 32,
    align: "center",
    defaultVisible: true,
    render: (r, { t }) =>
      r.coverCount > 0 ? (
        <span className="cover-dot" aria-label={t("columns.cover")} />
      ) : (
        <span className="cover-dot missing" title={t("badges.noCover")} aria-label={t("badges.noCover")} />
      ),
  },
  { id: "rating", sortField: "rating", width: 84, defaultVisible: true, render: (r) => <Stars value={r.rating} /> },
  { id: "mainArtist", sortField: "mainArtist", width: 180, defaultVisible: true, render: (r) => r.mainArtist ?? "" },
  { id: "title", sortField: "title", width: 240, defaultVisible: true, render: (r) => r.title ?? r.fileName },
  { id: "version", sortField: "version", width: 140, defaultVisible: true, render: (r) => r.version ?? "" },
  { id: "artists", sortField: "artists", width: 200, defaultVisible: false, render: (r) => r.artists.join(", ") },
  { id: "album", sortField: "album", width: 180, defaultVisible: true, render: (r) => r.album ?? "" },
  { id: "year", sortField: "year", width: 60, align: "right", defaultVisible: true, render: (r) => r.year ?? "" },
  { id: "genre", sortField: "genre", width: 120, defaultVisible: true, render: (r) => r.genre ?? "" },
  { id: "bpm", sortField: "bpm", width: 56, align: "right", defaultVisible: true, render: (r, c) => formatBpm(r.bpm, c.locale) },
  { id: "key", sortField: "key", width: 56, defaultVisible: true, render: (r, c) => formatKey(r.key, c.keyNotation) },
  { id: "format", sortField: "format", width: 64, defaultVisible: false, render: (r) => r.format.toUpperCase() },
  {
    id: "quality",
    sortField: "quality",
    width: 96,
    defaultVisible: true,
    render: (r, c) => (
      <span className={isLowBitrate(r) ? "text-warning" : undefined} title={isLowBitrate(r) ? c.t("badges.lowBitrate") : undefined}>
        {formatQuality(r, c.locale)}
      </span>
    ),
  },
  { id: "duration", sortField: "duration", width: 64, align: "right", defaultVisible: true, render: (r) => formatDuration(r.durationMs) },
  { id: "fileName", sortField: "fileName", width: 260, defaultVisible: false, render: (r) => r.fileName },
  { id: "status", width: 140, defaultVisible: true, render: (r, c) => <Badges row={r} ctx={c} /> },
];

export interface ColumnState {
  id: string;
  width: number;
  visible: boolean;
}

const STORAGE_KEY = "table.columns.v1";

export function loadColumnState(): ColumnState[] {
  const defaults = COLUMNS.map((c) => ({ id: c.id, width: c.width, visible: c.defaultVisible }));
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const saved = JSON.parse(raw) as ColumnState[];
    const known = saved.filter((s) => COLUMNS.some((c) => c.id === s.id));
    // Colonnes ajoutées dans une version plus récente : ajoutées à la fin.
    for (const d of defaults) if (!known.some((k) => k.id === d.id)) known.push(d);
    return known;
  } catch {
    return defaults;
  }
}

export function saveColumnState(state: ColumnState[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* simple confort */
  }
}
