import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Activity, CalendarX, FolderOpen, HardDriveDownload, History, Trash2, Waves } from "lucide-react";
import { inTauri, isCoreError } from "../core/api";
import { formatSize } from "../core/format";
import { pickFolder } from "../core/library";
import { analysisRun, backupRun, historyClear, historyImport, loudnessRun, type CopyReport } from "../core/djtools";
import { AnalysisDialog, DatesDialog } from "./Phase6Dialogs";

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
}

async function pickHistoryFile(title: string): Promise<string | null> {
  if (!inTauri) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ title, multiple: false, directory: false, filters: [{ name: "Rekordbox (XML, M3U8, TXT)", extensions: ["xml", "m3u8", "m3u", "txt"] }] });
  return typeof r === "string" ? r : null;
}

/** Réglages › Outils du DJ (phase 6). */
export function ToolsSettings({ values, onSettingChange, onToast }: Props) {
  const { t, i18n } = useTranslation();
  const [busy, setBusy] = useState<string | null>(null);
  const [report, setReport] = useState<CopyReport | null>(null);
  const [review, setReview] = useState(false);
  const [dates, setDates] = useState(false);
  const errText = (e: unknown) => (isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal"));

  const importHistory = async () => {
    const p = await pickHistoryFile(t("plays.import"));
    if (!p) return;
    setBusy("history");
    try {
      const r = await historyImport(p);
      onToast(t("plays.imported", { sessions: r.sessions, plays: r.plays, unmatched: r.unmatched }));
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const backup = async () => {
    const dest = values.backup_dest;
    if (!dest) return;
    setBusy("backup");
    setReport(null);
    try {
      const r = await backupRun(dest);
      setReport(r);
      onSettingChange("backup_last", new Date().toISOString().slice(0, 10));
      onToast(r.failures.length ? t("backup.doneWithFailures", { count: r.failures.length }) : t("backup.done", { copied: r.copied, skipped: r.skipped }), undefined, r.failures.length ? "error" : undefined);
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const date = (d: string) => (d ? new Date(d).toLocaleDateString(i18n.language, { dateStyle: "long" }) : t("backup.never"));

  return (
    <section>
      <h2>{t("tools.title")}</h2>
      <p className="hint">{t("tools.hint")}</p>

      <h3 className="form-label"><History size={14} aria-hidden /> {t("plays.title")}</h3>
      <p className="hint">{t("plays.hint")}</p>
      <div className="folder-actions">
        <button className="btn btn-small" disabled={busy != null} onClick={() => void importHistory()}>{t("plays.import")}</button>
        <button
          className="btn btn-small"
          disabled={busy != null}
          onClick={async () => {
            if (!window.confirm(t("plays.confirmClear"))) return;
            await historyClear().catch(() => {});
            onToast(t("plays.cleared"));
          }}
        >
          <Trash2 size={14} aria-hidden /> {t("plays.clear")}
        </button>
      </div>

      <h3 className="form-label"><HardDriveDownload size={14} aria-hidden /> {t("backup.title")}</h3>
      <p className="hint">{t("backup.hint")}</p>
      <div className="folder-actions">
        <button className="btn btn-small" onClick={async () => {
          const p = await pickFolder(t("backup.pick"));
          if (p) onSettingChange("backup_dest", p);
        }}>
          <FolderOpen size={14} aria-hidden /> {t("backup.pick")}
        </button>
        <span className="muted small">{values.backup_dest || t("backup.noDest")}</span>
      </div>
      <div className="folder-actions">
        <button className="btn btn-primary btn-small" disabled={!values.backup_dest || busy != null} onClick={() => void backup()}>
          {busy === "backup" ? t("backup.running") : t("backup.run")}
        </button>
        <span className="muted small">{t("backup.last", { date: date(values.backup_last ?? "") })}</span>
        <span className="spacer" />
        <label className="chip-select">
          {t("backup.remind")}
          <select value={values.backup_remind || "off"} onChange={(e) => onSettingChange("backup_remind", e.target.value)}>
            <option value="off">{t("backup.remindOff")}</option>
            {["7", "14", "30"].map((d) => (
              <option key={d} value={d}>{t("backup.remindDays", { count: Number(d) })}</option>
            ))}
          </select>
        </label>
      </div>
      {report && (
        <p className="small">
          {t("backup.report", { copied: report.copied, skipped: report.skipped, size: formatSize(report.bytes, i18n.language) })}
          {report.offline > 0 && <span className="text-warning"> {t("usb.offline", { count: report.offline })}</span>}
        </p>
      )}

      <h3 className="form-label"><Waves size={14} aria-hidden /> {t("loudness.title")}</h3>
      <p className="hint">{t("loudness.hint")}</p>
      <div className="folder-actions">
        <button className="btn btn-small" onClick={async () => onToast((await loudnessRun(null)) ? t("loudness.started") : t("loudness.already"))}>
          {t("loudness.run")}
        </button>
      </div>

      <h3 className="form-label"><Activity size={14} aria-hidden /> {t("analysis.title")}</h3>
      <p className="hint">{t("analysis.hint")}</p>
      <div className="folder-actions">
        <button className="btn btn-small" onClick={async () => onToast((await analysisRun(null)) ? t("analysis.started") : t("analysis.already"))}>
          {t("analysis.run")}
        </button>
        <button className="btn btn-small" onClick={() => setReview(true)}>{t("analysis.review")}</button>
      </div>

      <h3 className="form-label"><CalendarX size={14} aria-hidden /> {t("dates.title")}</h3>
      <p className="hint">{t("dates.settingsHint")}</p>
      <div className="folder-actions">
        <button className="btn btn-small" onClick={() => setDates(true)}>{t("dates.check")}</button>
      </div>

      {review && <AnalysisDialog onClose={() => setReview(false)} onDone={(r) => onToast(t("analysis.applied", { count: r.ok }))} />}
      {dates && <DatesDialog ids={null} onClose={() => setDates(false)} onDone={(r) => onToast(t("dates.repaired", { count: r.ok }))} />}
    </section>
  );
}
