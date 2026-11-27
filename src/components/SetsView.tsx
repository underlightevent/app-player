import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { startDrag, useDropTarget } from "../core/dnd";
import { useTranslation } from "react-i18next";
import { ArrowDown, ArrowUp, FileDown, GripVertical, ListOrdered, ListPlus, Pencil, Play, Plus, Trash2, Usb, X } from "lucide-react";
import { isCoreError } from "../core/api";
import { onEvent } from "../core/library";
import { formatDuration, formatKey } from "../core/format";
import {
  exportM3u,
  pickSave,
  playlistAdd,
  playlistDelete,
  playlistRemove,
  playlistUpdate,
  playlistsList,
  rekordboxExportXml,
  type Playlist,
} from "../core/rekordbox";
import { COMPAT_ICON, setCreate, setMove, setSuggestions, setTracks, type SetTrack, type Suggestion } from "../core/djtools";
import { UsbCopyDialog } from "./Phase6Dialogs";

interface Props {
  selection: number[];
  keyNotation: "camelot" | "musical";
  onPlay: (id: number) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
}

const COMPAT_CLASS: Record<string, string> = {
  same: "compat-good",
  adjacent: "compat-good",
  relative: "compat-good",
  energy: "compat-mid",
  clash: "compat-bad",
  unknown: "",
};

/** Ligne d'un set : cible du glisser-déposer interne (réordonner). */
function DropRow({ id, className, onDropRow, onClick, children }: { id: number; className: string; onDropRow: (dragged: number) => void; onClick: () => void; children: ReactNode }) {
  const ref = useDropTarget<HTMLTableRowElement>("setRow", (p) => {
    if (p.kind === "setRow" && p.id !== id) onDropRow(p.id);
  });
  return (
    <tr ref={ref} className={className} onClick={onClick}>
      {children}
    </tr>
  );
}

/** Écran Sets : listes ordonnées avec compatibilité des enchaînements et suggestions. */
export function SetsView({ selection, keyNotation, onPlay, onToast }: Props) {
  const { t, i18n } = useTranslation();
  const [sets, setSets] = useState<Playlist[] | null>(null);
  const [current, setCurrent] = useState<number | null>(null);
  const [tracks, setTracksState] = useState<SetTrack[]>([]);
  const [focus, setFocus] = useState<number | null>(null);
  const [sugs, setSugs] = useState<Suggestion[] | null>(null);
  const [range, setRange] = useState(6);
  const [newName, setNewName] = useState("");
  const [usb, setUsb] = useState(false);

  const errText = (e: unknown) => (isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal"));

  const loadSets = useCallback(async () => {
    const l = (await playlistsList()).filter((p) => p.kind === "set");
    setSets(l);
    setCurrent((c) => (c != null && l.some((p) => p.id === c) ? c : l[0]?.id ?? null));
  }, []);
  const loadTracks = useCallback(async () => {
    if (current == null) return setTracksState([]);
    setTracksState(await setTracks(current));
  }, [current]);
  useEffect(() => {
    void loadSets();
  }, [loadSets]);
  useEffect(() => {
    void loadTracks();
    return onEvent("library:changed", () => void loadTracks());
  }, [loadTracks]);

  // Suggestions pour le morceau choisi (sinon le dernier du set).
  const anchor = focus ?? tracks[tracks.length - 1]?.id ?? null;
  useEffect(() => {
    if (anchor == null) return setSugs(null);
    let alive = true;
    void setSuggestions(anchor, tracks.map((x) => x.id), range).then((s) => alive && setSugs(s));
    return () => {
      alive = false;
    };
  }, [anchor, tracks, range]);

  const set = sets?.find((p) => p.id === current) ?? null;
  const total = useMemo(() => tracks.reduce((a, x) => a + (x.durationMs ?? 0), 0), [tracks]);

  const create = async () => {
    if (!newName.trim()) return;
    const id = await setCreate(newName.trim());
    if (selection.length) await playlistAdd(id, selection);
    setNewName("");
    await loadSets();
    setCurrent(id);
  };
  const move = async (track: number, to: number) => {
    if (current == null) return;
    await setMove(current, track, Math.max(0, to));
    await loadTracks();
  };
  const remove = async (track: number) => {
    if (current == null) return;
    await playlistRemove(current, [track]);
    if (focus === track) setFocus(null);
    await loadTracks();
  };
  const add = async (ids: number[]) => {
    if (current == null || ids.length === 0) return;
    const n = await playlistAdd(current, ids);
    onToast(t("sets.added", { count: n }));
    await loadTracks();
  };
  const exportM3uSet = async () => {
    if (!set) return;
    const p = await pickSave(t("playlists.exportM3u"), `${set.name}.m3u8`, "m3u8", "M3U8");
    if (!p) return;
    try {
      const [n] = await exportM3u(tracks.map((x) => x.id), p);
      onToast(t("playlists.exported", { count: n }));
    } catch (e) {
      onToast(errText(e), undefined, "error");
    }
  };
  const exportXml = async () => {
    if (!set) return;
    const p = await pickSave(t("rekordbox.exportXml"), `${set.name}.xml`, "xml", "XML");
    if (!p) return;
    try {
      const [n] = await rekordboxExportXml([], [set.id], p);
      onToast(t("sets.xmlExported", { count: n }));
    } catch (e) {
      onToast(errText(e), undefined, "error");
    }
  };

  const keyText = (x: SetTrack) => formatKey(x.key, keyNotation);
  const diffText = (d: number | null) => (d == null ? "" : `${d > 0 ? "+" : ""}${d.toLocaleString(i18n.language, { maximumFractionDigits: 1 })} %`);
  const compat = (c: string | null) =>
    c ? (
      <span className={`compat ${COMPAT_CLASS[c] ?? ""}`} title={t(`sets.compat.${c}`)}>
        {COMPAT_ICON[c] ?? "?"}
      </span>
    ) : null;

  return (
    <div className="settings sets">
      <h1>{t("nav.sets")}</h1>
      <p className="note-info small">{t("sets.intro")}</p>
      <div className="sets-layout">
        <aside className="sets-list">
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
          >
            <input className="text-input" value={newName} placeholder={t("sets.newPlaceholder")} aria-label={t("sets.newPlaceholder")} onChange={(e) => setNewName(e.target.value)} />
            <button className="btn btn-small" type="submit" disabled={!newName.trim()} title={selection.length ? t("sets.createWith", { count: selection.length }) : undefined}>
              <Plus size={14} aria-hidden /> {t("sets.create")}
            </button>
          </form>
          {sets?.length === 0 && <p className="muted small">{t("sets.none")}</p>}
          <ul className="playlist-list">
            {sets?.map((p) => (
              <li key={p.id} className="playlist-row">
                <button className="playlist-open" aria-current={p.id === current} onClick={() => setCurrent(p.id)}>
                  <ListOrdered size={16} aria-hidden />
                  <span className="playlist-name">{p.name}</span>
                  <span className="muted small tabular">{p.count}</span>
                </button>
              </li>
            ))}
          </ul>
        </aside>

        <section className="sets-main">
          {!set ? (
            <div className="empty small-empty">
              <ListOrdered size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
              <p>{t("sets.empty")}</p>
            </div>
          ) : (
            <>
              <div className="folder-actions">
                <h2 className="sets-title">{set.name}</h2>
                <span className="muted small">
                  {t("sets.summary", { count: tracks.length, duration: formatDuration(total) })}
                </span>
                <span className="spacer" />
                <button className="icon-btn" title={t("sets.rename")} aria-label={t("sets.rename")} onClick={async () => {
                  const n = window.prompt(t("sets.rename"), set.name);
                  if (n && n.trim()) {
                    await playlistUpdate(set.id, n.trim(), null);
                    await loadSets();
                  }
                }}>
                  <Pencil size={16} aria-hidden />
                </button>
                <button className="icon-btn" title={t("sets.delete")} aria-label={t("sets.delete")} onClick={async () => {
                  if (!window.confirm(t("sets.confirmDelete", { name: set.name }))) return;
                  await playlistDelete(set.id);
                  await loadSets();
                }}>
                  <Trash2 size={16} aria-hidden />
                </button>
              </div>
              <div className="folder-actions">
                <button className="btn btn-small" disabled={selection.length === 0} onClick={() => void add(selection)}>
                  <ListPlus size={14} aria-hidden /> {t("sets.addSelection", { count: selection.length })}
                </button>
                <span className="spacer" />
                <button className="btn btn-small" disabled={!tracks.length} onClick={() => void exportM3uSet()}>
                  <FileDown size={14} aria-hidden /> M3U8
                </button>
                <button className="btn btn-small" disabled={!tracks.length} onClick={() => void exportXml()}>
                  <FileDown size={14} aria-hidden /> Rekordbox (XML)
                </button>
                <button className="btn btn-small" disabled={!tracks.length} onClick={() => setUsb(true)}>
                  <Usb size={14} aria-hidden /> {t("usb.action")}
                </button>
              </div>

              {tracks.length === 0 ? (
                <p className="muted">{t("sets.emptySet")}</p>
              ) : (
                <table className="plain-table set-table">
                  <thead>
                    <tr>
                      <th />
                      <th className="num">#</th>
                      <th />
                      <th>{t("columns.title")}</th>
                      <th className="num">{t("columns.bpm")}</th>
                      <th>{t("columns.key")}</th>
                      <th>{t("sets.transition")}</th>
                      <th className="num">{t("columns.energy")}</th>
                      <th className="num">{t("columns.duration")}</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {tracks.map((x, i) => (
                      <DropRow
                        key={x.id}
                        id={x.id}
                        className={focus === x.id ? "selected" : ""}
                        onDropRow={(dragged) => void move(dragged, i)}
                        onClick={() => setFocus(x.id)}
                      >
                        <td
                          className="drag-handle"
                          title={t("sets.dragHint")}
                          onMouseDown={(e) => startDrag(e, () => ({ kind: "setRow", id: x.id }), () => x.title ?? "")}
                        >
                          <GripVertical size={14} aria-hidden />
                        </td>
                        <td className="num tabular">{i + 1}</td>
                        <td>
                          <button className="icon-btn" aria-label={t("player.play")} onClick={(e) => { e.stopPropagation(); onPlay(x.id); }}>
                            <Play size={14} aria-hidden />
                          </button>
                        </td>
                        <td>
                          <div>{x.title}{x.version && <span className="muted"> ({x.version})</span>}</div>
                          <div className="muted small">{x.artist}</div>
                        </td>
                        <td className="num tabular">{x.bpm ? Math.round(x.bpm * 10) / 10 : ""}</td>
                        <td>{keyText(x)}</td>
                        <td className="small">
                          {i > 0 && (
                            <>
                              {compat(x.keyCompat)} <span className={x.bpmDiff != null && Math.abs(x.bpmDiff) > 6 ? "text-warning" : "muted"}>{diffText(x.bpmDiff)}</span>
                            </>
                          )}
                        </td>
                        <td className="num tabular">{x.energy ?? ""}</td>
                        <td className="num tabular">{formatDuration(x.durationMs)}</td>
                        <td className="watch-actions">
                          <button className="icon-btn" aria-label={t("sets.up")} disabled={i === 0} onClick={(e) => { e.stopPropagation(); void move(x.id, i - 1); }}>
                            <ArrowUp size={14} aria-hidden />
                          </button>
                          <button className="icon-btn" aria-label={t("sets.down")} disabled={i === tracks.length - 1} onClick={(e) => { e.stopPropagation(); void move(x.id, i + 1); }}>
                            <ArrowDown size={14} aria-hidden />
                          </button>
                          <button className="icon-btn" aria-label={t("sets.remove")} onClick={(e) => { e.stopPropagation(); void remove(x.id); }}>
                            <X size={14} aria-hidden />
                          </button>
                        </td>
                      </DropRow>
                    ))}
                  </tbody>
                </table>
              )}

              {anchor != null && (
                <div className="sets-suggestions">
                  <div className="folder-actions">
                    <h3 className="form-label">
                      {t("sets.suggestionsFor", { title: tracks.find((x) => x.id === anchor)?.title ?? "" })}
                    </h3>
                    <span className="spacer" />
                    <label className="chip-select">
                      {t("sets.bpmRange")}
                      <select value={range} onChange={(e) => setRange(Number(e.target.value))}>
                        {[3, 6, 10].map((r) => (
                          <option key={r} value={r}>±{r} %</option>
                        ))}
                      </select>
                    </label>
                  </div>
                  {sugs && sugs.length === 0 && <p className="muted small">{t("sets.noSuggestion")}</p>}
                  {sugs && sugs.length > 0 && (
                    <table className="plain-table set-table">
                      <tbody>
                        {sugs.map((s) => (
                          <tr key={s.id}>
                            <td>
                              <button className="icon-btn" aria-label={t("player.play")} onClick={() => onPlay(s.id)}>
                                <Play size={14} aria-hidden />
                              </button>
                            </td>
                            <td>
                              <div>{s.title}{s.version && <span className="muted"> ({s.version})</span>}</div>
                              <div className="muted small">{s.artist}{s.genre ? ` · ${s.genre}` : ""}</div>
                            </td>
                            <td className="num tabular">{s.bpm ? Math.round(s.bpm * 10) / 10 : ""}</td>
                            <td>{keyText(s)}</td>
                            <td className="small">
                              {compat(s.keyCompat)} <span className="muted">{diffText(s.bpmDiff)}</span>
                            </td>
                            <td className="watch-actions">
                              <button className="btn btn-small" onClick={() => void add([s.id])}>
                                <Plus size={14} aria-hidden /> {t("sets.addOne")}
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              )}
            </>
          )}
        </section>
      </div>
      {usb && set && <UsbCopyDialog ids={tracks.map((x) => x.id)} defaultName={set.name} onClose={() => setUsb(false)} onToast={onToast} />}
    </div>
  );
}
