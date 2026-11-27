import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileDown, FileUp, Info, ListRestart, RefreshCw, TriangleAlert, Upload } from "lucide-react";
import { onEvent } from "../core/library";
import {
  exportM3u,
  pickFile,
  pickSave,
  rekordboxImport,
  rekordboxJournalExport,
  rekordboxReloadClear,
  rekordboxStatus,
  updateCheck,
  openReleasePage,
  type RekordboxStatus,
} from "../core/rekordbox";
import { isCoreError } from "../core/api";
import { XmlExportDialog } from "./PlaylistsView";

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  /** Ouvre la bibliothèque filtrée : « in » (dans Rekordbox) ou « reload » (à recharger). */
  onOpenList: (kind: "in" | "reload") => void;
  selection: number[];
}

const today = () => new Date().toISOString().slice(0, 10);

/** Réglages › Rekordbox (import du XML, mode protégé, à recharger, journal, export) et mises à jour. */
export function RekordboxSettings({ values, onSettingChange, onToast, onOpenList, selection }: Props) {
  const { t, i18n } = useTranslation();
  const [s, setS] = useState<RekordboxStatus | null>(null);
  const [xml, setXml] = useState(false);
  const [onlyRb, setOnlyRb] = useState(true);
  const [checking, setChecking] = useState(false);
  const load = useCallback(async () => setS(await rekordboxStatus().catch(() => null)), []);
  useEffect(() => {
    void load();
    return onEvent("library:changed", () => void load());
  }, [load]);
  const date = new Intl.DateTimeFormat(i18n.language, { dateStyle: "long", timeStyle: "short" });

  const importXml = async () => {
    const path = await pickFile(t("rekordbox.importPick"), "xml", "XML");
    if (!path) return;
    try {
      const r = await rekordboxImport(path);
      onToast(t("rekordbox.imported", { matched: r.matched, total: r.xmlTracks }));
      await load();
    } catch (e) {
      onToast(isCoreError(e) ? t("rekordbox.importFailed", { detail: e.detail }) : t("errors.internal"), undefined, "error");
    }
  };

  const exportReload = async () => {
    const path = await pickSave(t("rekordbox.reloadExport"), `a-recharger-${today()}.m3u8`, "m3u8", "M3U8");
    if (!path) return;
    const [n] = await exportM3u(null, path);
    onToast(t("playlists.exported", { count: n }));
  };

  const exportJournal = async () => {
    const path = await pickSave(t("rekordbox.journalExport"), `journal-des-chemins-${today()}.csv`, "csv", "CSV");
    if (!path) return;
    await rekordboxJournalExport(path, onlyRb);
    onToast(t("rekordbox.journalExported"));
  };

  const check = async () => {
    setChecking(true);
    try {
      const u = await updateCheck();
      if (u.newer) onToast(t("update.available", { version: u.latest }), { label: t("update.download"), run: () => void openReleasePage(u.url) });
      else onToast(t("update.upToDate", { version: u.current }));
    } catch {
      onToast(t("update.failed"), undefined, "error");
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <section>
        <h2>Rekordbox</h2>
        <p className="hint">{t("rekordbox.intro")}</p>
        <div className="folder-actions">
          <button className="btn btn-primary" onClick={() => void importXml()}>
            <FileUp size={16} aria-hidden /> {s?.file ? t("rekordbox.reimport") : t("rekordbox.import")}
          </button>
          {s?.file && (
            <button className="btn" onClick={() => onOpenList("in")}>{t("rekordbox.showBadged", { count: s.matched })}</button>
          )}
        </div>
        {s?.file ? (
          <p className="small">
            {t("rekordbox.lastImport", { date: date.format(new Date((s.importedAt ?? 0) * 1000)), matched: s.matched, total: s.xmlTracks })}
            <br />
            <span className="muted">{s.file}</span>
          </p>
        ) : (
          <p className="muted small">{t("rekordbox.never")}</p>
        )}
        {s?.stale && (
          <p className="rekordbox-alert"><TriangleAlert size={16} aria-hidden /> {t("rekordbox.stale")}</p>
        )}
        <details className="help">
          <summary><Info size={14} aria-hidden /> {t("rekordbox.howExport")}</summary>
          <ol>
            <li>{t("rekordbox.howExport1")}</li>
            <li>{t("rekordbox.howExport2")}</li>
            <li>{t("rekordbox.howExport3")}</li>
          </ol>
        </details>

        <div className="checks">
          <label>
            <input type="checkbox" checked={values.rekordbox_protected !== "false"} onChange={(e) => onSettingChange("rekordbox_protected", String(e.target.checked))} />
            {t("rekordbox.protected")}
          </label>
        </div>
        <p className="hint small">{t("rekordbox.protectedHint")}</p>

        <h3 className="form-label">{t("rekordbox.reloadTitle", { count: s?.reloadCount ?? 0 })}</h3>
        <p className="hint small">{t("rekordbox.reloadHint")}</p>
        <div className="folder-actions">
          <button className="btn" disabled={!s?.reloadCount} onClick={() => onOpenList("reload")}>{t("rekordbox.reloadShow")}</button>
          <button className="btn" disabled={!s?.reloadCount} onClick={() => void exportReload()}>
            <FileDown size={16} aria-hidden /> {t("rekordbox.reloadExport")}
          </button>
          <button className="btn" disabled={!s?.reloadCount} onClick={() => void rekordboxReloadClear(null).then(load)}>
            <ListRestart size={16} aria-hidden /> {t("rekordbox.reloadClear")}
          </button>
        </div>

        <h3 className="form-label">{t("rekordbox.journalTitle", { count: s?.journalCount ?? 0 })}</h3>
        <p className="hint small">{t("rekordbox.journalHint")}</p>
        <div className="folder-actions">
          <label className="check">
            <input type="checkbox" checked={onlyRb} onChange={(e) => setOnlyRb(e.target.checked)} /> {t("rekordbox.journalOnly")}
          </label>
          <button className="btn" disabled={!s?.journalCount} onClick={() => void exportJournal()}>
            <FileDown size={16} aria-hidden /> {t("rekordbox.journalExport")}
          </button>
        </div>

        <h3 className="form-label">{t("rekordbox.exportXml")}</h3>
        <p className="hint small">{t("rekordbox.exportXmlIntro")}</p>
        <button className="btn" onClick={() => setXml(true)}>
          <Upload size={16} aria-hidden /> {t("rekordbox.exportXml")}
        </button>
        <p className="note-info"><Info size={14} aria-hidden /> {t("rekordbox.wavNote")}</p>
      </section>

      <section>
        <h2>{t("update.title")}</h2>
        <p className="hint">{t("update.hint")}</p>
        <div className="checks">
          <label>
            <input type="checkbox" checked={values.update_check !== "false"} onChange={(e) => onSettingChange("update_check", String(e.target.checked))} />
            {t("update.auto")}
          </label>
        </div>
        <button className="btn" disabled={checking} onClick={() => void check()}>
          <RefreshCw size={16} aria-hidden /> {t("update.checkNow")}
        </button>
      </section>
      {xml && <XmlExportDialog selection={selection} onClose={() => setXml(false)} onToast={onToast} />}
    </>
  );
}
