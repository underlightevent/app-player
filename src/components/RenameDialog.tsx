import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ArrowRight, Save } from "lucide-react";
import { Modal } from "./Modal";
import {
  DEFAULT_TEMPLATE,
  TEMPLATE_VARIABLES,
  renameApply,
  renamePreview,
  renameValidate,
  type OnConflict,
  type PlanItem,
  type SavedTemplate,
} from "../core/rename";
import type { EditReport } from "../core/edit";

interface Props {
  ids: number[];
  template: string;
  saved: SavedTemplate[];
  onSaveTemplates: (current: string, saved: SavedTemplate[]) => void;
  onClose: () => void;
  onDone: (r: EditReport) => void;
}

export function RenameDialog({ ids, template: initial, saved, onSaveTemplates, onClose, onDone }: Props) {
  const { t, i18n } = useTranslation();
  const [template, setTemplate] = useState(initial || DEFAULT_TEMPLATE);
  const [onConflict, setOnConflict] = useState<OnConflict>("suffix");
  const [plan, setPlan] = useState<PlanItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saveName, setSaveName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const nf = new Intl.NumberFormat(i18n.language);

  // Aperçu recalculé à chaque changement du modèle (léger délai pendant la frappe).
  useEffect(() => {
    let stale = false;
    setLoading(true);
    const h = window.setTimeout(async () => {
      const err = await renameValidate(template);
      if (stale) return;
      setError(err);
      if (err) {
        setPlan([]);
        setLoading(false);
        return;
      }
      try {
        const p = await renamePreview(ids, template, onConflict);
        if (!stale) setPlan(p);
      } catch (e) {
        if (!stale) setError(String((e as { detail?: string })?.detail ?? e));
      } finally {
        if (!stale) setLoading(false);
      }
    }, 250);
    return () => {
      stale = true;
      window.clearTimeout(h);
    };
  }, [template, onConflict, ids]);

  const counts = useMemo(() => {
    const c = { ok: 0, same: 0, problem: 0, long: 0, rekordbox: 0 };
    for (const p of plan) {
      if (p.status === "ok") c.ok++;
      if (p.status === "ok" && p.inRekordbox) c.rekordbox++;
      else if (p.status === "same") c.same++;
      else c.problem++;
      if (p.longPath) c.long++;
    }
    return c;
  }, [plan]);

  const insertVariable = (v: string) => {
    const el = inputRef.current;
    const token = `{${v}}`;
    if (!el) return setTemplate((s) => s + token);
    const start = el.selectionStart ?? template.length;
    const end = el.selectionEnd ?? template.length;
    const next = template.slice(0, start) + token + template.slice(end);
    setTemplate(next);
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(start + token.length, start + token.length);
    });
  };

  const apply = async () => {
    setBusy(true);
    try {
      const r = await renameApply(ids, template, onConflict);
      onSaveTemplates(template, saved);
      onDone(r);
      onClose();
    } catch (e) {
      setError(String((e as { detail?: string })?.detail ?? e));
    } finally {
      setBusy(false);
    }
  };

  const shown = plan.filter((p) => p.status !== "same").slice(0, 300);

  return (
    <Modal
      title={t("rename.title", { count: ids.length })}
      onClose={onClose}
      wide
      footer={
        <>
          <span className="muted small">
            {t("rename.summary", { ok: nf.format(counts.ok), same: nf.format(counts.same), problems: nf.format(counts.problem) })}
          </span>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" onClick={apply} disabled={busy || loading || !!error || counts.ok === 0}>
            {t("rename.apply", { count: counts.ok })}
          </button>
        </>
      }
    >
      <label className="form-label" htmlFor="tpl">{t("rename.template")}</label>
      <div className="template-row">
        <input id="tpl" ref={inputRef} className="text-input mono" value={template} onChange={(e) => setTemplate(e.target.value)} spellCheck={false} />
        {saved.length > 0 && (
          <select className="text-input" value="" onChange={(e) => e.target.value && setTemplate(e.target.value)} aria-label={t("rename.savedTemplates")}>
            <option value="">{t("rename.savedTemplates")}</option>
            {saved.map((s) => <option key={s.name} value={s.template}>{s.name}</option>)}
          </select>
        )}
      </div>
      <div className="chips-row" aria-label={t("rename.variables")}>
        {TEMPLATE_VARIABLES.map((v) => (
          <button key={v} className="chip" onClick={() => insertVariable(v)} title={t(`rename.var.${v}`)}>
            {`{${v}}`}
          </button>
        ))}
        <button className="chip" onClick={() => setTemplate(DEFAULT_TEMPLATE)}>{t("rename.reset")}</button>
      </div>
      <p className="hint small">{t("rename.optionalHint")}</p>
      {error && <p className="text-error small"><AlertTriangle size={14} aria-hidden /> {error}</p>}

      <fieldset className="radio-row">
        <legend>{t("rename.conflicts")}</legend>
        <label><input type="radio" checked={onConflict === "suffix"} onChange={() => setOnConflict("suffix")} /> {t("rename.conflictSuffix")}</label>
        <label><input type="radio" checked={onConflict === "skip"} onChange={() => setOnConflict("skip")} /> {t("rename.conflictSkip")}</label>
      </fieldset>
      {counts.rekordbox > 0 && (
        <p className="rekordbox-alert" role="alert"><AlertTriangle size={16} aria-hidden /> {t("rekordbox.renameAlert", { count: counts.rekordbox })}</p>
      )}
      {counts.long > 0 && <p className="text-warning small"><AlertTriangle size={14} aria-hidden /> {t("rename.longPaths", { count: counts.long })}</p>}

      <div className="preview-list" aria-busy={loading}>
        {shown.length === 0 && !loading && <p className="muted">{t("rename.nothing")}</p>}
        {shown.map((p) => (
          <div key={p.id} className={`preview-item status-${p.status}`}>
            <span className="preview-old" title={`${p.folder}/${p.oldName}`}>{p.oldName}</span>
            <ArrowRight size={14} aria-hidden />
            <span className="preview-new" title={p.newName ?? ""}>
              {p.newName ?? "—"}
              {p.suffixed && <span className="badge badge-warning">{t("rename.suffixed")}</span>}
              {p.status !== "ok" && <span className="badge badge-error">{t(`rename.status.${p.status}`)}</span>}
              {p.longPath && <span className="badge badge-warning">{t("rename.long")}</span>}
              {p.inRekordbox && p.status === "ok" && <span className="badge badge-rekordbox">Rekordbox</span>}
            </span>
          </div>
        ))}
        {plan.filter((p) => p.status !== "same").length > shown.length && <p className="muted small">{t("rename.more")}</p>}
      </div>

      <details className="save-template">
        <summary><Save size={14} aria-hidden /> {t("rename.saveAs")}</summary>
        <div className="template-row">
          <input className="text-input" value={saveName} placeholder={t("rename.templateName")} onChange={(e) => setSaveName(e.target.value)} />
          <button
            className="btn"
            disabled={!saveName.trim() || !!error}
            onClick={() => {
              const next = [...saved.filter((s) => s.name !== saveName.trim()), { name: saveName.trim(), template }];
              onSaveTemplates(template, next);
              setSaveName("");
            }}
          >
            {t("rename.save")}
          </button>
        </div>
      </details>
    </Modal>
  );
}
