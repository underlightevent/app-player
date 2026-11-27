import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { analysisRun } from "../core/djtools";
import { Activity, FolderSearch, HeartPulse, Wrench } from "lucide-react";
import { emptyQuery, libraryQuery, libraryRows, onEvent, pickFolder } from "../core/library";
import type { EditReport } from "../core/edit";
import { onlineSearch } from "../core/online";
import { healthReport, qualityAnalyze, relocateApply, relocatePreview, tracksSyncWav, type HealthReport, type Relocation } from "../core/quality";
import type { ViewId } from "./Sidebar";
import { Modal } from "./Modal";

interface Props {
  /** Ouvre la bibliothèque filtrée sur ce compteur. */
  onOpen: (key: string) => void;
  onGo: (v: ViewId) => void;
  /** Ouvre le renommage (avec aperçu) pour ces morceaux. */
  onRename: (ids: number[]) => void;
  onDone: (r: EditReport, key: string) => void;
  onToast: (text: string, action?: { label: string; run: () => void }, kind?: "error") => void;
}

/** Correction disponible pour chaque compteur (toujours avec aperçu ou validation). */
const FIX: Record<string, "online" | "rename" | "sync" | "relocate" | "review" | "duplicates" | "analyze" | "bpmKey"> = {
  // Phase 6 : analyse BPM / tonalité (résultats à valider dans Réglages › Outils du DJ).
  no_bpm: "bpmKey",
  no_key: "bpmKey",
  no_cover: "online",
  small_cover: "online",
  no_year: "online",
  no_genre: "online",
  unidentified: "online",
  review: "review",
  duplicates: "duplicates",
  bad_names: "rename",
  bad_chars: "rename",
  long_paths: "rename",
  wav_desync: "sync",
  missing: "relocate",
};

/** Tableau de bord « Santé de la bibliothèque » : compteurs cliquables et corrections. */
export function HealthView({ onOpen, onGo, onRename, onDone, onToast }: Props) {
  const { t, i18n } = useTranslation();
  const [report, setReport] = useState<HealthReport | null>(null);
  const [sync, setSync] = useState<{ ids: number[]; names: string[] } | null>(null);
  const [relocate, setRelocate] = useState<Relocation[] | null>(null);
  const nf = new Intl.NumberFormat(i18n.language);

  const load = useCallback(async () => setReport(await healthReport()), []);
  useEffect(() => {
    void load();
    return onEvent("library:changed", () => void load());
  }, [load]);

  const idsOf = (key: string) => libraryQuery({ ...emptyQuery, health: key });

  const fix = async (key: string) => {
    const kind = FIX[key];
    if (kind === "review" || kind === "duplicates") return onGo(kind);
    if (kind === "relocate") {
      const dir = await pickFolder(t("health.relocatePick"));
      if (!dir) return;
      const found = await relocatePreview(dir);
      if (found.length === 0) return onToast(t("health.relocateNone"));
      return setRelocate(found);
    }
    const ids = await idsOf(key);
    if (ids.length === 0) return;
    if (kind === "bpmKey") {
      const started = await analysisRun(ids);
      return onToast(started ? t("analysis.startedHealth") : t("analysis.already"), { label: t("analysis.openReview"), run: () => onGo("settings") });
    }
    if (kind === "online") {
      await onlineSearch(ids);
      onToast(t("health.onlineQueued", { count: ids.length }), { label: t("online.view"), run: () => onGo("review") });
    } else if (kind === "rename") {
      onRename(ids);
    } else if (kind === "sync") {
      const rows = await libraryRows(ids.slice(0, 200));
      setSync({ ids, names: rows.map((r) => r.fileName) });
    }
  };

  const analyze = async () => {
    const started = await qualityAnalyze(null);
    onToast(started ? t("health.analyzing") : t("health.alreadyAnalyzing"));
  };

  if (!report) return null;
  const problems = report.counters.filter((c) => c.count > 0).length;

  return (
    <div className="settings health">
      <h1>{t("nav.health")}</h1>
      <p className="hint">
        {t("health.summary", { total: nf.format(report.total), count: problems })}
        {report.quarantined > 0 && ` ${t("health.quarantined", { count: report.quarantined })}`}
      </p>
      <div className="folder-actions">
        <button className="btn" onClick={() => void analyze()}>
          <Activity size={16} aria-hidden /> {t("health.analyze")}
        </button>
        {report.notAnalyzed > 0 && <span className="muted small">{t("health.notAnalyzed", { count: report.notAnalyzed, formatted: nf.format(report.notAnalyzed) })}</span>}
      </div>
      {problems === 0 && (
        <div className="empty small-empty">
          <HeartPulse size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("health.allGood")}</p>
        </div>
      )}
      <ul className="health-grid">
        {report.counters.map((c) => (
          <li key={c.key} className={`health-card ${c.count > 0 ? "has" : ""}`}>
            <button className="health-open" onClick={() => onOpen(c.key)} disabled={c.count === 0} title={t("health.openList")}>
              <span className="health-count tabular">{nf.format(c.count)}</span>
              <span className="health-label">{t(`health.keys.${c.key}`)}</span>
            </button>
            {c.count > 0 && FIX[c.key] && (
              <button className="btn btn-small" onClick={() => void fix(c.key)}>
                {FIX[c.key] === "relocate" ? <FolderSearch size={14} aria-hidden /> : <Wrench size={14} aria-hidden />} {t(`health.fix.${FIX[c.key]}`)}
              </button>
            )}
          </li>
        ))}
      </ul>
      <p className="hint small">{t("health.analyzeHint")}</p>

      {sync && (
        <Modal
          title={t("health.syncTitle", { count: sync.ids.length })}
          onClose={() => setSync(null)}
          footer={
            <>
              <button className="btn" onClick={() => setSync(null)}>{t("common.cancel")}</button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  const ids = sync.ids;
                  setSync(null);
                  onDone(await tracksSyncWav(ids), "health.synced");
                  void load();
                }}
              >
                {t("health.syncConfirm", { count: sync.ids.length })}
              </button>
            </>
          }
        >
          <p className="hint small">{t("health.syncHint")}</p>
          <ul className="plain-list">
            {sync.names.map((n) => (
              <li key={n}>{n}</li>
            ))}
            {sync.ids.length > sync.names.length && <li className="muted">{t("health.andMore", { count: sync.ids.length - sync.names.length })}</li>}
          </ul>
        </Modal>
      )}

      {relocate && (
        <Modal
          wide
          title={t("health.relocateTitle", { count: relocate.length })}
          onClose={() => setRelocate(null)}
          footer={
            <>
              <button className="btn" onClick={() => setRelocate(null)}>{t("common.cancel")}</button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  const items = relocate;
                  setRelocate(null);
                  const n = await relocateApply(items);
                  onToast(t("health.relocated", { count: n }));
                  void load();
                }}
              >
                {t("health.relocateConfirm", { count: relocate.length })}
              </button>
            </>
          }
        >
          <p className="hint small">{t("health.relocateHint")}</p>
          <table className="plain-table">
            <thead>
              <tr>
                <th>{t("health.oldPath")}</th>
                <th>{t("health.newPath")}</th>
                <th>{t("health.method")}</th>
              </tr>
            </thead>
            <tbody>
              {relocate.map((r) => (
                <tr key={r.trackId}>
                  <td className="dup-path">{r.oldRel}</td>
                  <td className="dup-path">{r.newPath}</td>
                  <td>{t(`health.methods.${r.method}`)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Modal>
      )}
    </div>
  );
}
