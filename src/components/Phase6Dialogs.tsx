import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Check, FolderOpen, Minus, Plus, Trash2 } from "lucide-react";
import { Modal } from "./Modal";
import { isCoreError } from "../core/api";
import type { EditReport } from "../core/edit";
import { formatSize } from "../core/format";
import { pickFolder, type TrackRow } from "../core/library";
import {
  TAG_COLORS,
  analysisApply,
  analysisProposals,
  datesApply,
  datesPlan,
  tagApply,
  tagCreate,
  tagDelete,
  tagsList,
  usbCopy,
  type CopyLayout,
  type CopyReport,
  type Proposal,
  type RepairItem,
  type UserTag,
} from "../core/djtools";
import { tagColor } from "./table/columns";

type Toast = (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;

const errText = (t: TFunction, e: unknown) =>
  isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal");

/** Étiquettes de la sélection : cocher / décocher, créer, supprimer. */
export function TagsDialog({ rows, onClose, onToast }: { rows: TrackRow[]; onClose: () => void; onToast: Toast }) {
  const { t } = useTranslation();
  const [tags, setTags] = useState<UserTag[] | null>(null);
  const [state, setState] = useState<Map<number, number>>(new Map());
  const [name, setName] = useState("");
  const [color, setColor] = useState<string>("pink");
  const ids = rows.map((r) => r.id);
  const load = async () => setTags(await tagsList());
  useEffect(() => {
    void load();
    const m = new Map<number, number>();
    for (const r of rows) for (const g of r.userTags) m.set(g.id, (m.get(g.id) ?? 0) + 1);
    setState(m);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggle = async (g: UserTag) => {
    const all = (state.get(g.id) ?? 0) === rows.length;
    try {
      await tagApply(g.id, ids, !all);
      setState((s) => new Map(s).set(g.id, all ? 0 : rows.length));
      await load();
    } catch (e) {
      onToast(errText(t, e), undefined, "error");
    }
  };

  const create = async () => {
    try {
      const id = await tagCreate(name.trim(), color);
      await tagApply(id, ids, true);
      setState((s) => new Map(s).set(id, rows.length));
      setName("");
      await load();
    } catch (e) {
      onToast(errText(t, e), undefined, "error");
    }
  };

  const remove = async (g: UserTag) => {
    if (!window.confirm(t("tags.confirmDelete", { name: g.name, count: g.count }))) return;
    await tagDelete(g.id);
    await load();
  };

  return (
    <Modal title={t("tags.title", { count: rows.length })} onClose={onClose} footer={<button className="btn btn-primary" onClick={onClose}>{t("common.close")}</button>}>
      <p className="hint">{t("tags.hint")}</p>
      {tags && tags.length === 0 && <p className="muted">{t("tags.none")}</p>}
      <ul className="tag-list">
        {tags?.map((g) => {
          const n = state.get(g.id) ?? 0;
          const mark = n === rows.length && n > 0 ? "all" : n > 0 ? "some" : "none";
          return (
            <li key={g.id} className="tag-row">
              <button className="tag-toggle" aria-pressed={mark === "all"} onClick={() => void toggle(g)}>
                <span className={`tag-box ${mark}`} style={{ borderColor: tagColor(g.color), background: mark === "all" ? tagColor(g.color) : undefined }}>
                  {mark === "all" ? <Check size={12} aria-hidden /> : mark === "some" ? <Minus size={12} aria-hidden /> : null}
                </span>
                <span style={{ color: tagColor(g.color) }}>{g.name}</span>
                <span className="muted small tabular">{t("tags.count", { count: g.count })}</span>
              </button>
              <button className="icon-btn" title={t("tags.delete")} aria-label={t("tags.delete")} onClick={() => void remove(g)}>
                <Trash2 size={14} aria-hidden />
              </button>
            </li>
          );
        })}
      </ul>
      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) void create();
        }}
      >
        <input className="text-input" value={name} placeholder={t("tags.newPlaceholder")} aria-label={t("tags.newPlaceholder")} onChange={(e) => setName(e.target.value)} />
        <span className="color-picks" role="radiogroup" aria-label={t("tags.color")}>
          {TAG_COLORS.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={color === c}
              aria-label={c}
              className={`color-pick${color === c ? " on" : ""}`}
              style={{ background: tagColor(c) }}
              onClick={() => setColor(c)}
            />
          ))}
        </span>
        <button className="btn" type="submit" disabled={!name.trim()}>
          <Plus size={16} aria-hidden /> {t("tags.create")}
        </button>
      </form>
    </Modal>
  );
}

/** Copie de morceaux vers une clé USB (ou un dossier), vérifiée, avec playlist M3U8. */
export function UsbCopyDialog({ ids, defaultName, onClose, onToast }: { ids: number[]; defaultName: string; onClose: () => void; onToast: Toast }) {
  const { t, i18n } = useTranslation();
  const [dest, setDest] = useState("");
  const [layout, setLayout] = useState<CopyLayout>("playlist");
  const [name, setName] = useState(defaultName);
  const [m3u, setM3u] = useState(true);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<CopyReport | null>(null);

  const run = async () => {
    setBusy(true);
    try {
      const r = await usbCopy(ids, dest, layout, name || "Playlist", m3u);
      setReport(r);
      if (r.failures.length === 0) onToast(t("usb.done", { count: r.copied }));
    } catch (e) {
      onToast(errText(t, e), undefined, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("usb.title", { count: ids.length })}
      onClose={() => !busy && onClose()}
      footer={
        report ? (
          <button className="btn btn-primary" onClick={onClose}>{t("common.close")}</button>
        ) : (
          <>
            <button className="btn" onClick={onClose} disabled={busy}>{t("common.cancel")}</button>
            <button className="btn btn-primary" onClick={() => void run()} disabled={busy || !dest}>
              {busy ? t("usb.copying") : t("usb.copy")}
            </button>
          </>
        )
      }
    >
      {report ? (
        <>
          <p>{t("usb.report", { copied: report.copied, skipped: report.skipped, size: formatSize(report.bytes, i18n.language) })}</p>
          {report.offline > 0 && <p className="text-warning">{t("usb.offline", { count: report.offline })}</p>}
          {report.failures.length > 0 && (
            <ul className="small text-error">
              {report.failures.slice(0, 8).map((f, i) => (
                <li key={i}>{f.file} — {f.message}</li>
              ))}
            </ul>
          )}
        </>
      ) : (
        <>
          <p className="hint">{t("usb.hint")}</p>
          <div className="folder-actions">
            <button className="btn" onClick={async () => setDest((await pickFolder(t("usb.pick"))) ?? dest)}>
              <FolderOpen size={16} aria-hidden /> {t("usb.pick")}
            </button>
            <span className="muted small">{dest || t("usb.noDest")}</span>
          </div>
          <label className="chip-select">
            {t("usb.layout")}
            <select value={layout} onChange={(e) => setLayout(e.target.value as CopyLayout)}>
              <option value="playlist">{t("usb.layouts.playlist")}</option>
              <option value="flat">{t("usb.layouts.flat")}</option>
              <option value="folders">{t("usb.layouts.folders")}</option>
            </select>
          </label>
          <div className="form-row">
            <label className="form-label" htmlFor="usb-name">{t("usb.name")}</label>
            <input id="usb-name" className="text-input" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <label className="checks">
            <span>
              <input type="checkbox" checked={m3u} onChange={(e) => setM3u(e.target.checked)} /> {t("usb.m3u")}
            </span>
          </label>
        </>
      )}
    </Modal>
  );
}

/** Aperçu des BPM et tonalités trouvés par l'analyse, puis écriture dans les tags. */
export function AnalysisDialog({ onClose, onDone }: { onClose: () => void; onDone: (r: EditReport, key: string) => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<Proposal[] | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void analysisProposals().then((p) => {
      setItems(p);
      setChecked(new Set(p.map((x) => x.trackId)));
    });
  }, []);
  const apply = async () => {
    setBusy(true);
    try {
      onDone(await analysisApply([...checked]), "analysis.applied");
      onClose();
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={t("analysis.reviewTitle")}
      wide
      onClose={() => !busy && onClose()}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || checked.size === 0} onClick={() => void apply()}>
            {t("analysis.apply", { count: checked.size })}
          </button>
        </>
      }
    >
      <p className="hint">{t("analysis.reviewHint")}</p>
      {items && items.length === 0 ? (
        <p className="muted">{t("analysis.nothing")}</p>
      ) : (
        <table className="plain-table">
          <thead>
            <tr>
              <th>
                <input
                  type="checkbox"
                  aria-label={t("common.selectAll")}
                  checked={!!items?.length && checked.size === items.length}
                  onChange={(e) => setChecked(new Set(e.target.checked ? items?.map((x) => x.trackId) : []))}
                />
              </th>
              <th>{t("columns.title")}</th>
              <th className="num">{t("columns.bpm")}</th>
              <th>{t("columns.key")}</th>
              <th className="num">{t("columns.energy")}</th>
            </tr>
          </thead>
          <tbody>
            {items?.map((p) => (
              <tr key={p.trackId}>
                <td>
                  <input
                    type="checkbox"
                    aria-label={p.title}
                    checked={checked.has(p.trackId)}
                    onChange={(e) => {
                      const s = new Set(checked);
                      if (e.target.checked) s.add(p.trackId);
                      else s.delete(p.trackId);
                      setChecked(s);
                    }}
                  />
                </td>
                <td>{p.title}</td>
                <td className="num tabular">{p.bpm ?? <span className="muted">{p.currentBpm ?? ""}</span>}</td>
                <td>{p.key ?? <span className="muted">{p.currentKey ?? ""}</span>}</td>
                <td className="num tabular">{p.energy ?? ""}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Modal>
  );
}

/** Réparation des dates ID3 abîmées : aperçu, puis réparation annulable. */
export function DatesDialog({ ids, onClose, onDone }: { ids: number[] | null; onClose: () => void; onDone: (r: EditReport, key: string) => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<RepairItem[] | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void datesPlan(ids).then(setItems);
  }, [ids]);
  const apply = async () => {
    if (!items) return;
    setBusy(true);
    try {
      onDone(await datesApply(items.map((i) => i.trackId)), "dates.repaired");
      onClose();
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={t("dates.title")}
      onClose={() => !busy && onClose()}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || !items?.length} onClick={() => void apply()}>
            {t("dates.apply", { count: items?.length ?? 0 })}
          </button>
        </>
      }
    >
      <p className="hint">{t("dates.hint")}</p>
      {!items ? (
        <p className="muted">{t("dates.searching")}</p>
      ) : items.length === 0 ? (
        <p className="muted">{t("dates.nothing")}</p>
      ) : (
        <ul className="small">
          {items.map((i) => (
            <li key={i.trackId}>
              {i.fileName} <span className="muted">— {i.frames.join(", ")}</span>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}
