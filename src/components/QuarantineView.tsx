import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Info, RotateCcw, ShieldAlert, Trash2 } from "lucide-react";
import { onEvent } from "../core/library";
import type { EditReport } from "../core/edit";
import { quarantineEmpty, quarantineList, quarantineRestore, quarantineSummary, type QuarantineItem, type QuarantineSummary } from "../core/quality";
import { formatSize } from "../core/format";
import { Modal } from "./Modal";

interface Props {
  onDone: (r: EditReport, key: string) => void;
}

/** Écran Quarantaine : fichiers écartés, restauration en un clic, envoi à la corbeille après confirmation. */
export function QuarantineView({ onDone }: Props) {
  const { t, i18n } = useTranslation();
  const [items, setItems] = useState<QuarantineItem[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirm, setConfirm] = useState<{ ids: number[] | null; summary: QuarantineSummary } | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    const list = await quarantineList();
    setItems(list);
    setSelected((s) => new Set([...s].filter((id) => list.some((i) => i.id === id))));
  }, []);
  useEffect(() => {
    void load();
    return onEvent("library:changed", () => void load());
  }, [load]);

  const date = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" });
  const ids = [...selected];

  const restore = async (list: number[]) => {
    onDone(await quarantineRestore(list), "quarantine.restored");
    await load();
  };

  const askEmpty = async (list: number[] | null) => setConfirm({ ids: list, summary: await quarantineSummary(list) });

  const empty = async () => {
    if (!confirm) return;
    setBusy(true);
    try {
      // Pas d'annulation possible ici : les fichiers sont dans la corbeille du système.
      const r = await quarantineEmpty(confirm.ids);
      onDone({ ...r, batchId: null }, "quarantine.emptied");
      setConfirm(null);
      await load();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings quarantine">
      <h1>{t("nav.quarantine")}</h1>
      <p className="note-info"><Info size={14} aria-hidden /> {t("quarantine.intro")}</p>
      <div className="folder-actions">
        <button className="btn" disabled={ids.length === 0} onClick={() => void restore(ids)}>
          <RotateCcw size={16} aria-hidden /> {t("quarantine.restore", { count: ids.length })}
        </button>
        <span className="spacer" />
        <button className="btn btn-danger" disabled={!items?.length} onClick={() => void askEmpty(ids.length ? ids : null)}>
          <Trash2 size={16} aria-hidden /> {ids.length ? t("quarantine.trashSelected", { count: ids.length }) : t("quarantine.empty")}
        </button>
      </div>

      {items && items.length === 0 ? (
        <div className="empty small-empty">
          <ShieldAlert size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("quarantine.none")}</p>
        </div>
      ) : (
        <table className="plain-table">
          <thead>
            <tr>
              <th>
                <input
                  type="checkbox"
                  aria-label={t("common.selectAll")}
                  checked={!!items?.length && selected.size === items.length}
                  onChange={(e) => setSelected(new Set(e.target.checked ? items?.map((i) => i.id) : []))}
                />
              </th>
              <th>{t("quarantine.file")}</th>
              <th>{t("quarantine.origin")}</th>
              <th>{t("quarantine.reason")}</th>
              <th>{t("quarantine.date")}</th>
              <th className="num">{t("duplicates.size")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {items?.map((it) => (
              <tr key={it.id} className={!it.online ? "offline" : ""}>
                <td>
                  <input
                    type="checkbox"
                    aria-label={it.fileName}
                    checked={selected.has(it.id)}
                    onChange={(e) => {
                      const s = new Set(selected);
                      if (e.target.checked) s.add(it.id);
                      else s.delete(it.id);
                      setSelected(s);
                    }}
                  />
                </td>
                <td>
                  <div>{it.title ? `${it.artists ?? ""} – ${it.title}` : it.fileName}</div>
                  <div className="muted small">{it.fileName}</div>
                </td>
                <td className="dup-path" title={it.origRel}>
                  {it.volumeLabel} · {it.origRel}
                  {!it.online && <div className="muted small">{t("quarantine.offline")}</div>}
                  {it.online && !it.exists && <div className="muted small">{t("quarantine.gone")}</div>}
                </td>
                <td>{t(`quarantine.reasons.${it.reason ?? "manual"}`, { defaultValue: it.reason ?? "" })}</td>
                <td className="tabular">{date.format(new Date(it.addedAt * 1000))}</td>
                <td className="num tabular">{formatSize(it.size, i18n.language)}</td>
                <td>
                  <button className="btn btn-small" disabled={!it.online || !it.exists} onClick={() => void restore([it.id])}>
                    <RotateCcw size={14} aria-hidden /> {t("quarantine.restoreOne")}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirm && (
        <Modal
          title={t("quarantine.confirmTitle")}
          onClose={() => setConfirm(null)}
          footer={
            <>
              <button className="btn" onClick={() => setConfirm(null)}>{t("common.cancel")}</button>
              <button className="btn btn-danger" disabled={busy || confirm.summary.count === 0} onClick={() => void empty()}>
                <Trash2 size={16} aria-hidden /> {t("quarantine.confirmButton", { count: confirm.summary.count })}
              </button>
            </>
          }
        >
          <p>
            {t("quarantine.confirmText", {
              count: confirm.summary.count,
              size: formatSize(confirm.summary.size, i18n.language),
            })}
          </p>
          <p className="hint small">{t("quarantine.confirmHint")}</p>
          {confirm.summary.offline > 0 && <p className="hint small">{t("quarantine.confirmOffline", { count: confirm.summary.offline })}</p>}
        </Modal>
      )}
    </div>
  );
}
