import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderInput, Info, RefreshCw } from "lucide-react";
import { libraryRows, type RootInfo, type TrackRow } from "../core/library";
import { inboxFile, inboxList, onEventOnline, onlineSearch } from "../core/online";
import type { EditReport } from "../core/edit";
import { FolderTree, type FolderTarget } from "./FolderTree";
import { Modal } from "./Modal";

interface Props {
  roots: RootInfo[];
  lastTarget: string;
  onAddInbox: () => void;
  onDone: (r: EditReport, key: string) => void;
}

/** Inbox : nouveaux fichiers du dossier d'arrivée, traités automatiquement, puis rangés à la main. */
export function InboxView({ roots, lastTarget, onAddInbox, onDone }: Props) {
  const { t } = useTranslation();
  const [items, setItems] = useState<{ row: TrackRow; state: string }[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [picking, setPicking] = useState(false);
  const inboxRoots = roots.filter((r) => r.kind === "inbox");

  const load = useCallback(async () => {
    const list = await inboxList();
    const rows = await libraryRows(list.map(([id]) => id));
    const state = new Map(list);
    setItems(rows.map((row) => ({ row, state: state.get(row.id) ?? "pending" })));
  }, []);

  useEffect(() => {
    void load();
    const a = onEventOnline("online:update", () => void load());
    const b = onEventOnline("library:changed", () => void load());
    return () => {
      a();
      b();
    };
  }, [load]);

  if (inboxRoots.length === 0) {
    return (
      <div className="empty">
        <FolderInput size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
        <h1>{t("inbox.noFolder")}</h1>
        <p>{t("inbox.noFolderText")}</p>
        <button className="btn btn-primary" onClick={onAddInbox}>{t("inbox.choose")}</button>
      </div>
    );
  }

  const ready = items.filter((i) => i.state === "done").map((i) => i.row.id);
  const chosen = selected.size ? [...selected] : ready;
  const [lastVol, lastRel] = lastTarget.includes("|") ? [Number(lastTarget.split("|")[0]), lastTarget.split("|").slice(1).join("|")] : [null, ""];

  const file = async (f: FolderTarget) => {
    setPicking(false);
    onDone(await inboxFile(chosen, f.volumeId, f.relPath), "inbox.filed");
    setSelected(new Set());
    await load();
  };

  return (
    <div className="settings inbox">
      <h1>{t("nav.inbox")}</h1>
      <p className="note-info"><Info size={14} aria-hidden /> {t("inbox.rekordboxTip")}</p>
      <div className="folder-actions">
        <button className="btn" disabled={selected.size === 0} onClick={() => void onlineSearch([...selected])}>
          <RefreshCw size={16} aria-hidden /> {t("inbox.searchAgain")}
        </button>
        <button className="btn btn-primary" disabled={chosen.length === 0} onClick={() => setPicking(true)}>
          <FolderInput size={16} aria-hidden /> {t("inbox.file", { count: chosen.length })}
        </button>
        {lastVol != null && chosen.length > 0 && (
          <button className="btn" onClick={() => void file({ volumeId: lastVol, relPath: lastRel, name: lastRel.split("/").pop() ?? "" })}>
            {t("inbox.fileLast", { name: lastRel.split("/").pop() })}
          </button>
        )}
      </div>
      {items.length === 0 ? (
        <p className="muted">{t("inbox.empty")}</p>
      ) : (
        <ul className="inbox-list">
          {items.map(({ row, state }) => (
            <li key={row.id} className="inbox-item">
              <input
                type="checkbox"
                checked={selected.has(row.id)}
                onChange={(e) => {
                  const s = new Set(selected);
                  if (e.target.checked) s.add(row.id);
                  else s.delete(row.id);
                  setSelected(s);
                }}
                aria-label={row.fileName}
              />
              <span className="inbox-name">
                {row.title ? `${row.artists[0] ?? ""} – ${row.title}${row.version ? ` (${row.version})` : ""}` : row.fileName}
                <span className="muted small"> · {row.fileName}</span>
              </span>
              <span className={`badge ${state === "done" ? "badge-info" : state === "review" ? "badge-warning" : state === "unidentified" ? "badge-error" : ""}`}>
                {t(`inbox.state.${state}`, { defaultValue: state })}
              </span>
            </li>
          ))}
        </ul>
      )}
      {picking && (
        <Modal title={t("inbox.pickTarget")} onClose={() => setPicking(false)}>
          <p className="hint small">{t("inbox.pickHint")}</p>
          <FolderTree
            roots={roots}
            current={null}
            revision={0}
            onOpen={(f) => void file(f)}
            onDropTracks={() => {}}
            onCreate={() => {}}
            onRename={() => {}}
            onReveal={() => {}}
          />
        </Modal>
      )}
    </div>
  );
}
