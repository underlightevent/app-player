import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, CheckCheck, ImageOff, Search, X } from "lucide-react";
import {
  ONLINE_FIELDS,
  onlineAccept,
  onlineCoverFetch,
  onlineCoverPreviews,
  onlineManualSearch,
  onlineReject,
  onlineResults,
  onEventOnline,
  type Candidate,
  type CoverPreview,
  type ResultRow,
} from "../core/online";
import type { CoverData, EditReport } from "../core/edit";
import { coverPrepare } from "../core/edit";
import type { TagValues } from "../core/rename";
import { CropEditor } from "./CropEditor";
import { Modal } from "./Modal";

type Current = TagValues & { rating?: number };

function show(v: unknown): string {
  if (Array.isArray(v)) return v.join(", ");
  return v == null ? "" : String(v);
}

function value(c: Candidate | Current, f: string): unknown {
  return (c as unknown as Record<string, unknown>)[f];
}

/** Champs cochés par défaut : ceux que le candidat change. */
export function defaultFields(current: Current, c: Candidate): string[] {
  return ONLINE_FIELDS.filter((f) => {
    const n = show(value(c, f));
    return n !== "" && n !== show(value(current, f));
  });
}

/** Comparaison champ par champ, choix du candidat et de la pochette. */
export function CandidateCompare(p: {
  current: Current;
  candidates: Candidate[];
  chosen: number;
  onChoose: (i: number) => void;
  fields: string[];
  onFields: (f: string[]) => void;
  hasCover: boolean;
  cover: CoverPreview | "none" | null;
  onCover: (c: CoverPreview | "none") => void;
  previews: CoverPreview[];
  loadingCovers: boolean;
}) {
  const { t } = useTranslation();
  const c = p.candidates[p.chosen];
  return (
    <div className="compare">
      <div className="candidates" role="listbox" aria-label={t("review.candidates")}>
        {p.candidates.map((cand, i) => (
          <button key={`${cand.source}-${cand.id}-${i}`} role="option" aria-selected={i === p.chosen} className={`candidate ${i === p.chosen ? "on" : ""}`} onClick={() => p.onChoose(i)}>
            <span className="cand-num tabular">{i + 1}</span>
            <span className="cand-main">
              <strong>{cand.artists[0] ?? "?"} – {cand.title ?? "?"}</strong>
              {cand.version && <span className="muted"> ({cand.version})</span>}
              <span className="cand-sub muted small">
                {[cand.album, cand.year, t(`sources.${cand.source}`)].filter(Boolean).join(" · ")}
              </span>
              <span className="cand-sub muted small">{cand.scoreDetail.join(" · ")}</span>
            </span>
            <span className={`score-badge ${cand.score >= 90 ? "high" : cand.score >= 60 ? "mid" : "low"}`}>{cand.score}</span>
          </button>
        ))}
      </div>
      {c && (
        <table className="field-compare">
          <thead>
            <tr>
              <th />
              <th>{t("review.field")}</th>
              <th>{t("review.current")}</th>
              <th>{t("review.proposed")}</th>
            </tr>
          </thead>
          <tbody>
            {ONLINE_FIELDS.map((f) => {
              const cur = show(value(p.current, f));
              const prop = show(value(c, f));
              const differs = prop !== "" && prop !== cur;
              const checked = p.fields.includes(f);
              return (
                <tr key={f} className={differs ? "differs" : ""}>
                  <td>
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={prop === ""}
                      aria-label={t(`fields.${f}`)}
                      onChange={(e) => p.onFields(e.target.checked ? [...p.fields, f] : p.fields.filter((x) => x !== f))}
                    />
                  </td>
                  <td className="muted">{t(`fields.${f}`)}</td>
                  <td className={checked && differs ? "struck" : ""}>{cur || "—"}</td>
                  <td className={differs ? "proposed" : "muted"}>{prop || "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      <div className="cover-grid-title small muted">
        {t("review.covers")}
        {p.loadingCovers && ` · ${t("review.loadingCovers")}`}
      </div>
      <div className="cover-grid">
        <button className={`cover-choice ${p.cover === "none" ? "on" : ""}`} onClick={() => p.onCover("none")}>
          <span className="cover-none"><ImageOff size={24} aria-hidden /></span>
          <span className="small">{p.hasCover ? t("review.keepCover") : t("review.noCover")}</span>
        </button>
        {p.previews.map((pv) => (
          <button key={pv.url} className={`cover-choice ${p.cover !== "none" && p.cover?.url === pv.url ? "on" : ""}`} onClick={() => p.onCover(pv)}>
            <img src={pv.preview} alt="" />
            <span className="small tabular">
              {pv.width} × {pv.height}
              {pv.width < 500 ? ` · ${t("review.small")}` : ""}
            </span>
            <span className="small muted">{t(`sources.${pv.source}`)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

/** État partagé d'une comparaison : candidat, champs cochés, pochettes, recadrage. */
function useCompareState(current: Current | null, candidates: Candidate[], hasCover: boolean, key: string) {
  const [chosen, setChosen] = useState(0);
  const [fields, setFields] = useState<string[]>([]);
  const [previews, setPreviews] = useState<CoverPreview[]>([]);
  const [loadingCovers, setLoadingCovers] = useState(false);
  const [cover, setCover] = useState<CoverPreview | "none" | null>(null);

  useEffect(() => {
    setChosen(0);
    setPreviews([]);
    setCover(null);
  }, [key]);

  useEffect(() => {
    const c = candidates[chosen];
    if (current && c) setFields(defaultFields(current, c));
  }, [current, candidates, chosen]);

  useEffect(() => {
    if (candidates.length === 0) return;
    let stale = false;
    setLoadingCovers(true);
    onlineCoverPreviews(candidates.slice(0, 5))
      .then((p) => {
        if (stale) return;
        setPreviews(p);
        // Présélection : sans pochette actuelle, la plus grande et la plus carrée.
        setCover((cur) => cur ?? (hasCover || p.length === 0 ? "none" : p[0]));
      })
      .catch(() => {})
      .finally(() => !stale && setLoadingCovers(false));
    return () => {
      stale = true;
    };
  }, [candidates, hasCover]);

  return { chosen, setChosen, fields, setFields, previews, loadingCovers, cover, setCover };
}

/** Pochette choisie : écrite telle quelle si elle est (presque) carrée, sinon passage par l'éditeur. */
async function resolveCover(cover: CoverPreview | "none" | null, openCrop: (src: string) => Promise<CoverData | null>): Promise<{ url: string | null; data: CoverData | null } | null> {
  if (!cover || cover === "none") return { url: null, data: null };
  const ratio = Math.min(cover.width, cover.height) / Math.max(cover.width, cover.height, 1);
  if (ratio >= 0.9) return { url: cover.url, data: null };
  const src = await onlineCoverFetch(cover.url, cover.source);
  const data = await openCrop(src);
  return data ? { url: null, data } : null;
}

export function ReviewView({ onDone }: { onDone: (r: EditReport) => void }) {
  const { t, i18n } = useTranslation();
  const [tab, setTab] = useState<"review" | "unidentified" | "auto">("review");
  const [rows, setRows] = useState<ResultRow[]>([]);
  const [all, setAll] = useState<ResultRow[]>([]);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [manual, setManual] = useState<{ artist: string; title: string } | null>(null);
  const [extra, setExtra] = useState<Record<number, Candidate[]>>({});
  const [crop, setCrop] = useState<{ src: string; resolve: (c: CoverData | null) => void } | null>(null);
  const nf = new Intl.NumberFormat(i18n.language);

  const load = useCallback(async () => {
    const [list, everything] = await Promise.all([onlineResults(tab === "auto" ? "auto" : tab), onlineResults("all")]);
    setRows(list);
    setAll(everything);
    setIndex((i) => Math.min(i, Math.max(0, list.length - 1)));
  }, [tab]);

  useEffect(() => {
    void load();
    return onEventOnline("online:update", () => void load());
  }, [load]);

  const row = rows[index] ?? null;
  const candidates = useMemo(() => (row ? [...(extra[row.trackId] ?? []), ...row.candidates] : []), [row, extra]);
  const st = useCompareState(row?.current ?? null, candidates, (row?.coverCount ?? 0) > 0, `${row?.trackId}-${candidates.length}`);

  const openCrop = (src: string) => new Promise<CoverData | null>((resolve) => setCrop({ src, resolve }));

  const accept = useCallback(async () => {
    if (!row || busy) return;
    const c = candidates[st.chosen];
    if (!c) return;
    setBusy(true);
    try {
      const cv = await resolveCover(st.cover, openCrop);
      if (!cv) return;
      onDone(await onlineAccept(row.trackId, c, st.fields, cv.url, cv.data));
      await load();
    } finally {
      setBusy(false);
    }
  }, [row, busy, candidates, st.chosen, st.cover, st.fields, onDone, load]);

  const reject = useCallback(async () => {
    if (!row) return;
    await onlineReject(row.trackId);
    await load();
  }, [row, load]);

  // Morceaux de la même sortie (même artiste et même album) : acceptation groupée.
  const sameRelease = row?.releaseGroup ? rows.filter((r) => r.releaseGroup === row.releaseGroup) : [];
  const acceptRelease = async () => {
    setBusy(true);
    try {
      let ok = 0;
      for (const r of sameRelease) {
        const c = r.candidates[0];
        if (!c) continue;
        const cover = r.coverCount === 0 ? c.covers.find((x) => x.source === "coverartarchive")?.url ?? null : null;
        const res = await onlineAccept(r.trackId, c, defaultFields(r.current, c), cover, null);
        ok += res.ok;
      }
      onDone({ batchId: null, ok, failures: [] });
      await load();
    } finally {
      setBusy(false);
    }
  };

  // Raccourcis : Entrée accepte, Échap rejette, flèches changent de morceau, 1 à 9 choisissent un candidat.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (crop || manual || (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT"))) return;
      if (e.key === "Enter") {
        e.preventDefault();
        void accept();
      } else if (e.key === "Escape") {
        e.preventDefault();
        void reject();
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        setIndex((i) => Math.min(rows.length - 1, i + 1));
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setIndex((i) => Math.max(0, i - 1));
      } else if (/^[1-9]$/.test(e.key) && !e.ctrlKey && !e.metaKey) {
        const n = Number(e.key) - 1;
        if (n < candidates.length) {
          e.preventDefault();
          e.stopPropagation();
          st.setChosen(n);
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [accept, reject, rows.length, candidates.length, crop, manual, st]);

  // Statistiques : taux d'identification sur tout ce qui a été recherché.
  const stats = useMemo(() => {
    const c = { auto: 0, review: 0, unidentified: 0, accepted: 0, total: 0 };
    for (const r of all) {
      if (r.status === "pending") continue;
      c.total++;
      if (r.status === "auto") c.auto++;
      else if (r.status === "review") c.review++;
      else if (r.status === "unidentified" || r.status === "error") c.unidentified++;
      else if (r.status === "accepted") c.accepted++;
    }
    return c;
  }, [all]);

  return (
    <div className="review">
      <div className="review-head">
        <div className="segmented" role="tablist">
          {(["review", "unidentified", "auto"] as const).map((k) => (
            <button key={k} role="tab" aria-pressed={tab === k} onClick={() => { setTab(k); setIndex(0); }}>
              {t(`review.tab.${k}`)}
            </button>
          ))}
        </div>
        {stats.total > 0 && (
          <span className="muted small tabular">
            {t("review.stats", {
              total: nf.format(stats.total),
              auto: nf.format(stats.auto),
              review: nf.format(stats.review),
              accepted: nf.format(stats.accepted),
              unidentified: nf.format(stats.unidentified),
              rate: Math.round(((stats.auto + stats.accepted) / stats.total) * 100),
            })}
          </span>
        )}
      </div>

      {rows.length === 0 ? (
        <div className="empty">
          <CheckCheck size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <h1>{t(`review.empty.${tab}`)}</h1>
          <p>{t("review.emptyHint")}</p>
        </div>
      ) : (
        <div className="review-body">
          <ul className="review-list" role="listbox" aria-label={t("nav.review")}>
            {rows.map((r, i) => (
              <li key={r.trackId}>
                <button className={`review-item ${i === index ? "on" : ""}`} role="option" aria-selected={i === index} onClick={() => setIndex(i)}>
                  <span className="review-file">{r.current.title ? `${r.current.artists[0] ?? ""} – ${r.current.title}` : r.fileName}</span>
                  <span className={`score-badge ${r.score >= 90 ? "high" : r.score >= 60 ? "mid" : "low"}`}>{r.score}</span>
                </button>
              </li>
            ))}
          </ul>
          {row && (
            <div className="review-detail">
              <div className="review-title">
                <strong>{row.fileName}</strong>
                {row.applied.length > 0 && <span className="muted small"> · {t("review.applied", { fields: row.applied.map((f) => t(`fields.${f}`)).join(", ") })}</span>}
              </div>
              {candidates.length === 0 ? (
                <p className="muted">{t("review.noCandidate")}</p>
              ) : (
                <CandidateCompare current={row.current} candidates={candidates} chosen={st.chosen} onChoose={st.setChosen} fields={st.fields} onFields={st.setFields} hasCover={row.coverCount > 0} cover={st.cover} onCover={st.setCover} previews={st.previews} loadingCovers={st.loadingCovers} />
              )}
              <div className="review-actions">
                <button className="btn" onClick={() => setManual({ artist: row.current.artists[0] ?? "", title: row.current.title ?? "" })}>
                  <Search size={16} aria-hidden /> {t("review.manual")}
                </button>
                <span style={{ flex: 1 }} />
                {sameRelease.length > 1 && (
                  <button className="btn" disabled={busy} onClick={() => void acceptRelease()}>
                    <CheckCheck size={16} aria-hidden /> {t("review.acceptRelease", { count: sameRelease.length })}
                  </button>
                )}
                <button className="btn" disabled={busy} onClick={() => void reject()} title={t("review.rejectHint")}>
                  <X size={16} aria-hidden /> {t("review.reject")}
                </button>
                <button className="btn btn-primary" disabled={busy || candidates.length === 0 || st.fields.length === 0 && st.cover === "none"} onClick={() => void accept()} title={t("review.acceptHint")}>
                  <Check size={16} aria-hidden /> {t("review.accept")}
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {manual && row && (
        <ManualSearchDialog
          trackId={row.trackId}
          initial={manual}
          onClose={() => setManual(null)}
          onResults={(c) => {
            setExtra((x) => ({ ...x, [row.trackId]: c }));
            setManual(null);
          }}
        />
      )}
      {crop && (
        <CropEditor
          src={crop.src}
          onCancel={() => {
            crop.resolve(null);
            setCrop(null);
          }}
          onConfirm={async (edit) => {
            const data = crop.src.split(",")[1];
            try {
              const p = await coverPrepare({ data }, edit);
              crop.resolve(p.cover);
            } catch {
              crop.resolve(null);
            }
            setCrop(null);
          }}
        />
      )}
    </div>
  );
}

/** Recherche libre (artiste, titre) dans toutes les sources activées. */
export function ManualSearchDialog({ trackId, initial, onClose, onResults }: { trackId: number; initial: { artist: string; title: string }; onClose: () => void; onResults: (c: Candidate[]) => void }) {
  const { t } = useTranslation();
  const [artist, setArtist] = useState(initial.artist);
  const [title, setTitle] = useState(initial.title);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await onlineManualSearch(trackId, artist, title);
      if (r.length === 0) setError(t("review.nothingFound"));
      else onResults(r);
    } catch (e) {
      const code = (e as { code?: string })?.code;
      setError(code === "offline" ? t("review.offline") : t("review.searchFailed"));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={t("review.manual")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || (!artist.trim() && !title.trim())} onClick={() => void run()}>
            <Search size={16} aria-hidden /> {t("review.searchButton")}
          </button>
        </>
      }
    >
      <label className="form-label" htmlFor="ms-artist">{t("fields.artists")}</label>
      <input id="ms-artist" className="text-input" style={{ width: "100%" }} value={artist} onChange={(e) => setArtist(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void run()} />
      <label className="form-label" htmlFor="ms-title">{t("fields.title")}</label>
      <input id="ms-title" className="text-input" style={{ width: "100%" }} value={title} onChange={(e) => setTitle(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void run()} />
      {error && <p className="text-error small">{error}</p>}
    </Modal>
  );
}

/** Recherche manuelle depuis le panneau de détail : résultats comparés et appliqués d'un clic. */
export function ManualSearchPanel({ trackId, current, hasCover, onClose, onDone }: { trackId: number; current: Current; hasCover: boolean; onClose: () => void; onDone: (r: EditReport) => void }) {
  const { t } = useTranslation();
  const [candidates, setCandidates] = useState<Candidate[] | null>(null);
  const [crop, setCrop] = useState<{ src: string; resolve: (c: CoverData | null) => void } | null>(null);
  const st = useCompareState(current, candidates ?? [], hasCover, `${trackId}-${candidates?.length ?? 0}`);
  const [busy, setBusy] = useState(false);
  if (!candidates) {
    return <ManualSearchDialog trackId={trackId} initial={{ artist: current.artists[0] ?? "", title: current.title ?? "" }} onClose={onClose} onResults={setCandidates} />;
  }
  return (
    <>
      <Modal
        title={t("review.manualResults")}
        onClose={onClose}
        wide
        footer={
          <>
            <button className="btn" onClick={() => setCandidates(null)}>{t("review.searchAgain")}</button>
            <span style={{ flex: 1 }} />
            <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
            <button
              className="btn btn-primary"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  const cv = await resolveCover(st.cover, (src) => new Promise((resolve) => setCrop({ src, resolve })));
                  if (!cv) return;
                  onDone(await onlineAccept(trackId, candidates[st.chosen], st.fields, cv.url, cv.data));
                  onClose();
                } finally {
                  setBusy(false);
                }
              }}
            >
              <Check size={16} aria-hidden /> {t("review.apply")}
            </button>
          </>
        }
      >
        <CandidateCompare current={current} candidates={candidates} chosen={st.chosen} onChoose={st.setChosen} fields={st.fields} onFields={st.setFields} hasCover={hasCover} cover={st.cover} onCover={st.setCover} previews={st.previews} loadingCovers={st.loadingCovers} />
      </Modal>
      {crop && (
        <CropEditor
          src={crop.src}
          onCancel={() => {
            crop.resolve(null);
            setCrop(null);
          }}
          onConfirm={async (edit) => {
            try {
              const p = await coverPrepare({ data: crop.src.split(",")[1] }, edit);
              crop.resolve(p.cover);
            } catch {
              crop.resolve(null);
            }
            setCrop(null);
          }}
        />
      )}
    </>
  );
}
