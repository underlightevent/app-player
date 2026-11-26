import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ClipboardPaste, Disc3, Globe, ImagePlus, Info, Lock, LockOpen, Star, Trash2, X } from "lucide-react";
import type { TrackRow } from "../core/library";
import type { Changes, EditableField } from "../core/edit";
import { trackCover } from "../core/edit";
import { formatBpm, formatDuration, formatKey, formatQuality, formatSize } from "../core/format";
import { startDrag, useDropTarget } from "../core/dnd";

interface Props {
  selected: TrackRow[];
  selectionSize: number;
  keyNotation: "camelot" | "musical";
  genres: string[];
  onEdit: (changes: Changes) => void;
  onLock: (field: EditableField, locked: boolean) => void;
  onPickCover: () => void;
  onPasteCover: () => void;
  /** Change à chaque modification du morceau : relance le chargement de la pochette. */
  revision: number;
  /** Nom de fichier que donnerait le modèle de renommage. */
  futureName: string | null;
  /** Recherche manuelle en ligne pour le morceau sélectionné. */
  onManualSearch: () => void;
}

/** Valeur commune à toute la sélection, ou MIXED si elles diffèrent. */
const MIXED = Symbol("mixed");
type Common<T> = T | typeof MIXED;

function common<T>(rows: TrackRow[], get: (r: TrackRow) => T): Common<T> {
  if (rows.length === 0) return MIXED;
  const first = JSON.stringify(get(rows[0]));
  return rows.every((r) => JSON.stringify(get(r)) === first) ? get(rows[0]) : MIXED;
}

function LockButton({ field, rows, onLock }: { field: EditableField; rows: TrackRow[]; onLock: Props["onLock"] }) {
  const { t } = useTranslation();
  const locked = rows.some((r) => r.locked.includes(field));
  const label = locked ? t("detail.unlock") : t("detail.lock");
  return (
    <button className={`lock-btn ${locked ? "on" : ""}`} onClick={() => onLock(field, !locked)} title={label} aria-label={`${label} : ${t(`fields.${field}`)}`} aria-pressed={locked}>
      {locked ? <Lock size={14} aria-hidden /> : <LockOpen size={14} aria-hidden />}
    </button>
  );
}

/** Champ texte : valide à Entrée ou en quittant le champ, Échap annule. */
function TextField(props: {
  field: EditableField;
  value: Common<string | null>;
  rows: TrackRow[];
  onCommit: (v: string | null) => void;
  onLock: Props["onLock"];
  type?: "text" | "number";
  list?: string;
  display?: (v: string) => string;
  inputMode?: "numeric" | "decimal" | "text";
}) {
  const { t } = useTranslation();
  const initial = props.value === MIXED ? "" : props.display ? props.display(props.value ?? "") : (props.value ?? "");
  const [text, setText] = useState(initial);
  const [dirty, setDirty] = useState(false);
  useEffect(() => {
    setText(initial);
    setDirty(false);
  }, [initial]);
  const commit = () => {
    if (!dirty) return;
    setDirty(false);
    if (text.trim() === initial.trim()) return;
    props.onCommit(text.trim() === "" ? null : text.trim());
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
      (e.target as HTMLInputElement).blur();
    } else if (e.key === "Escape") {
      setText(initial);
      setDirty(false);
      (e.target as HTMLInputElement).blur();
    }
  };
  const id = `f-${props.field}`;
  return (
    <div className="edit-field">
      <label htmlFor={id}>{t(`fields.${props.field}`)}</label>
      <input
        id={id}
        value={text}
        list={props.list}
        inputMode={props.inputMode}
        placeholder={props.value === MIXED ? t("detail.mixed") : ""}
        onChange={(e) => {
          setText(e.target.value);
          setDirty(true);
        }}
        onBlur={commit}
        onKeyDown={onKey}
        spellCheck={false}
      />
      <LockButton field={props.field} rows={props.rows} onLock={props.onLock} />
    </div>
  );
}

/** Étiquette d'artiste : glisser pour réordonner, étoile pour en faire l'artiste principal. */
function ArtistTag(p: { name: string; index: number; onMove: (from: number, to: number) => void; onRemove: () => void }) {
  const { t } = useTranslation();
  const ref = useDropTarget<HTMLSpanElement>("artist", (payload) => {
    if (payload.kind === "artist") p.onMove(payload.index, p.index);
  });
  const main = p.index === 0;
  return (
    <span
      ref={ref}
      className={`artist-tag ${main ? "main" : ""}`}
      onMouseDown={(e) => {
        if ((e.target as HTMLElement).closest("button")) return;
        startDrag(e, () => ({ kind: "artist", index: p.index }), () => p.name);
      }}
      title={t("detail.artistDragHint")}
    >
      <button
        className="tag-btn"
        onClick={() => p.onMove(p.index, 0)}
        aria-label={main ? t("detail.mainArtist") : t("detail.makeMain", { name: p.name })}
        title={main ? t("detail.mainArtist") : t("detail.makeMain", { name: p.name })}
      >
        <Star size={11} className={main ? "star on" : "star"} aria-hidden />
      </button>
      {p.name}
      <button className="tag-btn" onClick={p.onRemove} aria-label={t("detail.removeArtist", { name: p.name })}>
        <X size={11} aria-hidden />
      </button>
    </span>
  );
}

function ArtistsField({ rows, onCommit, onLock }: { rows: TrackRow[]; onCommit: (a: string[]) => void; onLock: Props["onLock"] }) {
  const { t } = useTranslation();
  const value = common(rows, (r) => r.artists);
  const artists = value === MIXED ? null : value;
  const [adding, setAdding] = useState("");
  const commit = (next: string[]) => onCommit(next);
  const move = (from: number, to: number) => {
    if (!artists || from === to) return;
    const next = [...artists];
    const [x] = next.splice(from, 1);
    next.splice(to, 0, x);
    commit(next);
  };
  return (
    <div className="edit-field artists-field">
      <label>{t("fields.artists")}</label>
      <div className="artist-tags">
        {artists === null && <span className="muted">{t("detail.mixed")}</span>}
        {artists?.map((a, i) => (
          <ArtistTag key={a + i} name={a} index={i} onMove={move} onRemove={() => commit(artists.filter((_, j) => j !== i))} />
        ))}
        <input
          className="artist-add"
          value={adding}
          placeholder={t("detail.addArtist")}
          onChange={(e) => setAdding(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && adding.trim()) {
              e.preventDefault();
              commit([...(artists ?? []), adding.trim()]);
              setAdding("");
            }
          }}
        />
      </div>
      <LockButton field="artists" rows={rows} onLock={onLock} />
    </div>
  );
}

function RatingField({ rows, onCommit, onLock }: { rows: TrackRow[]; onCommit: (n: number) => void; onLock: Props["onLock"] }) {
  const { t } = useTranslation();
  const value = common(rows, (r) => r.rating);
  const [hover, setHover] = useState<number | null>(null);
  const shown = hover ?? (value === MIXED ? 0 : value);
  return (
    <div className="edit-field">
      <label>{t("fields.rating")}</label>
      <div className="rating-input" role="radiogroup" aria-label={t("fields.rating")} onMouseLeave={() => setHover(null)}>
        {[1, 2, 3, 4, 5].map((n) => (
          <button
            key={n}
            role="radio"
            aria-checked={value === n}
            aria-label={t("detail.stars", { count: n })}
            onMouseEnter={() => setHover(n)}
            onClick={() => onCommit(value === n ? 0 : n)}
          >
            <Star size={16} className={n <= shown ? "star on" : "star"} aria-hidden />
          </button>
        ))}
        {value === MIXED && <span className="muted small">{t("detail.mixed")}</span>}
      </div>
      <LockButton field="rating" rows={rows} onLock={onLock} />
    </div>
  );
}

function CoverBox(props: { rows: TrackRow[]; revision: number; onPick: () => void; onPaste: () => void; onRemove: () => void }) {
  const { t } = useTranslation();
  const [src, setSrc] = useState<string | null>(null);
  const single = props.rows.length === 1 ? props.rows[0] : null;
  useEffect(() => {
    setSrc(null);
    if (!single || single.coverCount === 0 || !single.online) return;
    let stale = false;
    void trackCover(single.id).then((s) => {
      if (!stale) setSrc(s);
    });
    return () => {
      stale = true;
    };
  }, [single?.id, single?.coverCount, single?.online, props.revision]);
  const anyCover = props.rows.some((r) => r.coverCount > 0);
  return (
    <div className="cover-box" data-cover-drop>
      {src ? <img src={src} alt={t("fields.cover")} /> : (
        <div className="cover-placeholder" aria-hidden>
          <Disc3 size={48} strokeWidth={1.25} />
        </div>
      )}
      <div className="cover-actions">
        <button className="icon-btn" onClick={props.onPick} title={t("detail.coverPick")} aria-label={t("detail.coverPick")}>
          <ImagePlus size={18} aria-hidden />
        </button>
        <button className="icon-btn" onClick={props.onPaste} title={t("detail.coverPaste")} aria-label={t("detail.coverPaste")}>
          <ClipboardPaste size={18} aria-hidden />
        </button>
        {anyCover && (
          <button className="icon-btn" onClick={props.onRemove} title={t("detail.coverRemove")} aria-label={t("detail.coverRemove")}>
            <Trash2 size={18} aria-hidden />
          </button>
        )}
      </div>
      <div className="cover-hint">{t("detail.coverDropHint")}</div>
    </div>
  );
}

export function DetailPanel(p: Props) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const rows = p.selected;
  const scrollRef = useRef<HTMLElement>(null);

  if (p.selectionSize === 0) {
    return (
      <aside className="detail" aria-label={t("detail.title")}>
        <h2 className="detail-title">{t("detail.title")}</h2>
        <div className="cover-placeholder" aria-hidden>
          <Disc3 size={48} strokeWidth={1.25} />
        </div>
        <p>{t("detail.empty")}</p>
        <p className="muted">{t("detail.hint")}</p>
      </aside>
    );
  }
  if (rows.length === 0) return <aside className="detail" aria-label={t("detail.title")} aria-busy />;

  const offline = rows.some((r) => !r.online);
  const single = rows.length === 1 ? rows[0] : null;
  const edit = (c: Changes) => p.onEdit(c);
  const parseNum = (v: string | null, int: boolean) => {
    if (v == null) return null;
    const n = Number(v.replace(",", "."));
    return Number.isFinite(n) ? (int ? Math.round(n) : n) : undefined;
  };
  const info = (label: string, value: ReactNode) =>
    value === "" || value == null ? null : (
      <div className="field">
        <dt>{label}</dt>
        <dd>{value}</dd>
      </div>
    );

  return (
    <aside className="detail" aria-label={t("detail.title")} ref={scrollRef}>
      <CoverBox rows={rows} revision={p.revision} onPick={p.onPickCover} onPaste={p.onPasteCover} onRemove={() => edit({ cover: null })} />
      {rows.length > 1 && <p className="detail-count">{t("detail.multiple", { count: p.selectionSize })}</p>}
      {offline && <p className="note-info"><Info size={14} aria-hidden /> {t("detail.offlineReadOnly")}</p>}
      {single?.format === "wav" && <p className="note-info"><Info size={14} aria-hidden /> {t("badges.wav")}</p>}
      {single?.status === "unreadable" && <p className="note-info text-error">{single.error ?? t("badges.unreadable")}</p>}

      <fieldset className="edit-form" disabled={offline}>
        <TextField field="title" rows={rows} value={common(rows, (r) => r.title)} onCommit={(v) => edit({ title: v })} onLock={p.onLock} />
        <TextField field="version" rows={rows} value={common(rows, (r) => r.version)} onCommit={(v) => edit({ version: v })} onLock={p.onLock} />
        <ArtistsField rows={rows} onCommit={(a) => edit({ artists: a })} onLock={p.onLock} />
        <TextField field="album" rows={rows} value={common(rows, (r) => r.album)} onCommit={(v) => edit({ album: v })} onLock={p.onLock} />
        <div className="edit-row">
          <TextField field="year" rows={rows} inputMode="numeric" value={common(rows, (r) => (r.year == null ? null : String(r.year)))} onCommit={(v) => { const n = parseNum(v, true); if (n !== undefined) edit({ year: n }); }} onLock={p.onLock} />
          <TextField field="bpm" rows={rows} inputMode="decimal" value={common(rows, (r) => (r.bpm == null ? null : formatBpm(r.bpm, locale)))} onCommit={(v) => { const n = parseNum(v, false); if (n !== undefined) edit({ bpm: n }); }} onLock={p.onLock} />
        </div>
        <div className="edit-row">
          <TextField field="genre" rows={rows} list="genre-list" value={common(rows, (r) => r.genre)} onCommit={(v) => edit({ genre: v })} onLock={p.onLock} />
          <TextField field="key" rows={rows} value={common(rows, (r) => (r.key ? formatKey(r.key, p.keyNotation) : null))} onCommit={(v) => edit({ key: v })} onLock={p.onLock} />
        </div>
        <datalist id="genre-list">
          {p.genres.map((g) => <option key={g} value={g} />)}
        </datalist>
        <RatingField rows={rows} onCommit={(n) => edit({ rating: n })} onLock={p.onLock} />
      </fieldset>

      {single && single.online && (
        <button className="btn detail-search" onClick={p.onManualSearch}>
          <Globe size={16} aria-hidden /> {t("detail.manualSearch")}
        </button>
      )}

      {single && (
        <>
          <h3 className="detail-section">{t("detail.technical")}</h3>
          <dl className="fields tabular">
            {info(t("detail.quality"), formatQuality(single, locale))}
            {info(t("detail.duration"), formatDuration(single.durationMs))}
            {info(t("detail.size"), formatSize(single.size, locale))}
            {info(t("detail.channels"), single.channels)}
            {info(t("detail.cover"), single.coverCount > 0 ? `${single.coverWidth ?? "?"} px${single.coverCount > 1 ? ` · ${t("detail.coverCount", { count: single.coverCount })}` : ""}` : t("badges.noCover"))}
            {info(t("detail.file"), <span className="path">{single.path ?? `${single.volumeLabel}/${single.relPath}`}</span>)}
            {p.futureName && p.futureName !== single.fileName && info(t("detail.futureName"), <span className="path">{p.futureName}</span>)}
          </dl>
        </>
      )}
    </aside>
  );
}
