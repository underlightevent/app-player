import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import { Modal } from "./Modal";
import {
  PARSE_PRESETS,
  cleanApply,
  cleanPreview,
  defaultCleanRules,
  parseApply,
  parsePreview,
  type CleanRules,
  type CustomRule,
  type TagPreview,
  type TagValues,
} from "../core/rename";
import type { EditReport } from "../core/edit";

const FIELDS: (keyof TagValues)[] = ["title", "version", "artists", "album", "genre", "year", "bpm", "key"];

function show(v: TagValues, f: keyof TagValues): string {
  const x = v[f];
  if (Array.isArray(x)) return x.join(", ");
  return x == null ? "" : String(x);
}

function PreviewList({ items, loading }: { items: TagPreview[]; loading: boolean }) {
  const { t } = useTranslation();
  if (!loading && items.length === 0) return <p className="muted">{t("clean.nothing")}</p>;
  return (
    <div className="preview-list" aria-busy={loading}>
      {items.slice(0, 300).map((p) => (
        <div key={p.id} className="tag-preview">
          <div className="tag-preview-file">{p.fileName}</div>
          {FIELDS.filter((f) => p.fields.includes(f)).map((f) => (
            <div key={f} className="tag-preview-field">
              <span className="muted">{t(`fields.${f}`)}</span>
              <del>{show(p.before, f) || "—"}</del>
              <ins>{show(p.after, f) || "—"}</ins>
            </div>
          ))}
        </div>
      ))}
      {items.length > 300 && <p className="muted small">{t("rename.more")}</p>}
    </div>
  );
}

interface CleanProps {
  ids: number[];
  initial: CleanRules | null;
  onSaveRules: (r: CleanRules) => void;
  onClose: () => void;
  onDone: (r: EditReport) => void;
}

export function CleanDialog({ ids, initial, onSaveRules, onClose, onDone }: CleanProps) {
  const { t } = useTranslation();
  const [rules, setRules] = useState<CleanRules>(initial ?? defaultCleanRules);
  const [items, setItems] = useState<TagPreview[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let stale = false;
    setLoading(true);
    const h = window.setTimeout(() => {
      cleanPreview(ids, rules)
        .then((p) => {
          if (!stale) {
            setItems(p);
            setError(null);
          }
        })
        .catch((e) => !stale && setError(String(e?.detail ?? e)))
        .finally(() => !stale && setLoading(false));
    }, 250);
    return () => {
      stale = true;
      window.clearTimeout(h);
    };
  }, [ids, rules]);

  const toggles: (keyof Omit<CleanRules, "custom">)[] = ["junk", "trackNumbers", "underscores", "featToArtists", "extractVersion", "versionCase", "titleCase"];
  const setCustom = (i: number, patch: Partial<CustomRule>) =>
    setRules((r) => ({ ...r, custom: r.custom.map((c, j) => (j === i ? { ...c, ...patch } : c)) }));

  return (
    <Modal
      title={t("clean.title", { count: ids.length })}
      onClose={onClose}
      wide
      footer={
        <>
          <span className="muted small">{t("clean.summary", { count: items.length })}</span>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={busy || loading || !!error || items.length === 0}
            onClick={async () => {
              setBusy(true);
              try {
                onSaveRules(rules);
                onDone(await cleanApply(ids, rules));
                onClose();
              } catch (e) {
                setError(String((e as { detail?: string })?.detail ?? e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("clean.apply", { count: items.length })}
          </button>
        </>
      }
    >
      <div className="checks">
        {toggles.map((k) => (
          <label key={k}>
            <input type="checkbox" checked={rules[k]} onChange={(e) => setRules((r) => ({ ...r, [k]: e.target.checked }))} /> {t(`clean.rule.${k}`)}
          </label>
        ))}
      </div>
      <h3 className="form-label">{t("clean.custom")}</h3>
      {rules.custom.map((c, i) => (
        <div key={i} className="custom-rule">
          <input className="text-input" value={c.find} placeholder={t("clean.find")} onChange={(e) => setCustom(i, { find: e.target.value })} />
          <input className="text-input" value={c.replace} placeholder={t("clean.replace")} onChange={(e) => setCustom(i, { replace: e.target.value })} />
          <label className="small"><input type="checkbox" checked={c.regex} onChange={(e) => setCustom(i, { regex: e.target.checked })} /> {t("clean.regex")}</label>
          <button className="icon-btn" onClick={() => setRules((r) => ({ ...r, custom: r.custom.filter((_, j) => j !== i) }))} aria-label={t("clean.removeRule")}>
            <Trash2 size={16} aria-hidden />
          </button>
        </div>
      ))}
      <button className="btn" onClick={() => setRules((r) => ({ ...r, custom: [...r.custom, { find: "", replace: "", regex: false, fields: [] }] }))}>
        <Plus size={16} aria-hidden /> {t("clean.addRule")}
      </button>
      <p className="hint small">{t("clean.lockedHint")}</p>
      {error && <p className="text-error small">{error}</p>}
      <PreviewList items={items} loading={loading} />
    </Modal>
  );
}

interface ParseProps {
  ids: number[];
  initial: string;
  onSavePattern: (p: string) => void;
  onClose: () => void;
  onDone: (r: EditReport) => void;
}

export function ParseDialog({ ids, initial, onSavePattern, onClose, onDone }: ParseProps) {
  const { t } = useTranslation();
  const [pattern, setPattern] = useState(initial || PARSE_PRESETS[0]);
  const [overwrite, setOverwrite] = useState(false);
  const [items, setItems] = useState<TagPreview[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let stale = false;
    setLoading(true);
    const h = window.setTimeout(() => {
      parsePreview(ids, pattern, overwrite)
        .then((p) => {
          if (!stale) {
            setItems(p);
            setError(null);
          }
        })
        .catch((e) => !stale && setError(String(e?.detail ?? e)))
        .finally(() => !stale && setLoading(false));
    }, 250);
    return () => {
      stale = true;
      window.clearTimeout(h);
    };
  }, [ids, pattern, overwrite]);

  return (
    <Modal
      title={t("parse.title", { count: ids.length })}
      onClose={onClose}
      wide
      footer={
        <>
          <span className="muted small">{t("clean.summary", { count: items.length })}</span>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={busy || loading || !!error || items.length === 0}
            onClick={async () => {
              setBusy(true);
              try {
                onSavePattern(pattern);
                onDone(await parseApply(ids, pattern, overwrite));
                onClose();
              } catch (e) {
                setError(String((e as { detail?: string })?.detail ?? e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("clean.apply", { count: items.length })}
          </button>
        </>
      }
    >
      <label className="form-label" htmlFor="pattern">{t("parse.pattern")}</label>
      <div className="template-row">
        <input id="pattern" className="text-input mono" value={pattern} onChange={(e) => setPattern(e.target.value)} spellCheck={false} />
      </div>
      <div className="chips-row">
        {PARSE_PRESETS.map((p) => (
          <button key={p} className="chip" aria-pressed={pattern === p} onClick={() => setPattern(p)}>{p}</button>
        ))}
      </div>
      <p className="hint small">{t("parse.hint")}</p>
      <label className="small"><input type="checkbox" checked={overwrite} onChange={(e) => setOverwrite(e.target.checked)} /> {t("parse.overwrite")}</label>
      {error && <p className="text-error small">{error}</p>}
      <PreviewList items={items} loading={loading} />
    </Modal>
  );
}
