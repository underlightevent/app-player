import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Folder, FolderPlus, HardDrive, RefreshCw, Trash2, Unplug } from "lucide-react";
import { libraryRemoveRoot, libraryRescan, type RootInfo } from "../core/library";

interface Props {
  roots: RootInfo[];
  onAddFolder: () => void;
  onChanged: () => void;
  onToast: (msg: string) => void;
}

/** Dossiers de la bibliothèque. L'arborescence complète et les déplacements arrivent au lot E. */
export function FoldersView({ roots, onAddFolder, onChanged, onToast }: Props) {
  const { t, i18n } = useTranslation();
  const [confirm, setConfirm] = useState<RootInfo | null>(null);
  const nf = new Intl.NumberFormat(i18n.language);

  const remove = async (r: RootInfo) => {
    setConfirm(null);
    try {
      await libraryRemoveRoot(r.id);
      onToast(t("folders.removed"));
      onChanged();
    } catch {
      onToast(t("errors.internal"));
    }
  };

  return (
    <div className="settings folders">
      <h1>{t("folders.title")}</h1>
      <p className="hint">{t("folders.hint")}</p>
      <div className="folder-actions">
        <button className="btn btn-primary" onClick={onAddFolder}>
          <FolderPlus size={16} aria-hidden />
          {t("views.library.action")}
        </button>
        <button className="btn" onClick={() => void libraryRescan()}>
          <RefreshCw size={16} aria-hidden />
          {t("folders.rescan")}
        </button>
      </div>
      <ul className="folder-list">
        {roots.map((r) => (
          <li key={r.id} className={`folder-item ${r.online ? "" : "offline"}`}>
            <Folder size={20} aria-hidden />
            <div className="folder-main">
              <div className="folder-path" title={r.path ?? r.relPath}>
                {r.path ?? `${r.volumeLabel}/${r.relPath}`}
              </div>
              <div className="folder-meta">
                {r.online ? <HardDrive size={12} aria-hidden /> : <Unplug size={12} aria-hidden />}
                {r.online ? r.volumeLabel : t("library.diskOffline", { disk: r.volumeLabel })}
                {" · "}
                {t("library.count", { count: r.trackCount, formatted: nf.format(r.trackCount) })}
              </div>
            </div>
            <button className="icon-btn" onClick={() => setConfirm(r)} aria-label={t("folders.remove")} title={t("folders.remove")}>
              <Trash2 size={16} aria-hidden />
            </button>
          </li>
        ))}
      </ul>

      {confirm && (
        <>
          <div className="menu-backdrop dim" onClick={() => setConfirm(null)} />
          <div className="dialog" role="alertdialog" aria-modal aria-labelledby="rm-title">
            <h2 id="rm-title">{t("folders.removeTitle")}</h2>
            <p>{t("folders.removeText", { path: confirm.path ?? confirm.relPath })}</p>
            <div className="dialog-actions">
              <button className="btn" onClick={() => setConfirm(null)} autoFocus>
                {t("common.cancel")}
              </button>
              <button className="btn btn-primary" onClick={() => void remove(confirm)}>
                {t("folders.remove")}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
