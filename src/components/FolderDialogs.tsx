import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ArrowRight } from "lucide-react";
import { Modal } from "./Modal";
import { tracksMove, tracksMovePreview } from "../core/folders";
import type { PlanItem } from "../core/rename";
import type { EditReport } from "../core/edit";
import type { FolderTarget } from "./FolderTree";

/** Aperçu puis confirmation du déplacement de morceaux vers un dossier. */
export function MoveDialog({ ids, target, onClose, onDone }: { ids: number[]; target: FolderTarget; onClose: () => void; onDone: (r: EditReport) => void }) {
  const { t } = useTranslation();
  const [plan, setPlan] = useState<PlanItem[] | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void tracksMovePreview(ids, target.volumeId, target.relPath).then(setPlan).catch(() => setPlan([]));
  }, [ids, target]);
  const ok = plan?.filter((p) => p.status === "ok").length ?? 0;
  const problems = plan?.filter((p) => !["ok", "same"].includes(p.status)) ?? [];
  return (
    <Modal
      title={t("move.title", { count: ids.length, folder: target.name })}
      onClose={onClose}
      wide
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={busy || ok === 0}
            onClick={async () => {
              setBusy(true);
              try {
                onDone(await tracksMove(ids, target.volumeId, target.relPath));
                onClose();
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("move.apply", { count: ok })}
          </button>
        </>
      }
    >
      <p className="hint">{t("move.hint")}</p>
      {(plan?.filter((p) => p.status === "ok" && p.inRekordbox).length ?? 0) > 0 && (
        <p className="rekordbox-alert" role="alert">
          <AlertTriangle size={16} aria-hidden /> {t("rekordbox.renameAlert", { count: plan!.filter((p) => p.status === "ok" && p.inRekordbox).length })}
        </p>
      )}
      {problems.length > 0 && (
        <p className="text-warning small">
          <AlertTriangle size={14} aria-hidden /> {t("move.problems", { count: problems.length })}
        </p>
      )}
      <div className="preview-list" aria-busy={plan == null}>
        {plan?.filter((p) => p.status !== "same").slice(0, 300).map((p) => (
          <div key={p.id} className={`preview-item status-${p.status}`}>
            <span className="preview-old" title={`${p.folder}/${p.oldName}`}>{p.folder}/{p.oldName}</span>
            <ArrowRight size={14} aria-hidden />
            <span className="preview-new" title={p.newName ?? ""}>
              {p.newName}
              {p.status !== "ok" && <span className="badge badge-error">{t(`move.status.${p.status}`)}</span>}
              {p.inRekordbox && p.status === "ok" && <span className="badge badge-rekordbox">Rekordbox</span>}
            </span>
          </div>
        ))}
      </div>
    </Modal>
  );
}

/** Saisie d'un nom (nouveau dossier, renommage). */
export function NameDialog({ title, initial, onClose, onSubmit }: { title: string; initial: string; onClose: () => void; onSubmit: (name: string) => Promise<void> }) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    if (!name.trim()) return;
    try {
      await onSubmit(name.trim());
      onClose();
    } catch (e) {
      setError(String((e as { detail?: string })?.detail ?? e));
    }
  };
  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!name.trim() || name.trim() === initial} onClick={submit}>{t("common.ok")}</button>
        </>
      }
    >
      <input
        className="text-input"
        style={{ width: "100%" }}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
        }}
      />
      {error && <p className="text-error small">{error}</p>}
    </Modal>
  );
}
