import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileDown, ListMusic, Pencil, Plus, Sparkles, Trash2, Upload, X } from "lucide-react";
import { onEvent } from "../core/library";
import {
  exportM3u,
  pickSave,
  playlistCreate,
  playlistDelete,
  playlistsList,
  playlistTrackIds,
  playlistUpdate,
  rekordboxExportXml,
  RULE_FIELDS,
  RULE_OPS,
  type Playlist,
  type Rule,
  type Rules,
} from "../core/rekordbox";
import { Modal } from "./Modal";
import { NameDialog } from "./FolderDialogs";

interface Props {
  onOpen: (p: Playlist) => void;
  onToast: (text: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  /** Morceaux sélectionnés dans la bibliothèque (pour l'export XML de la sélection). */
  selection: number[];
}

const today = () => new Date().toISOString().slice(0, 10);
const safeName = (s: string) => s.replace(/[<>:"/\\|?*]/g, "-").trim() || "playlist";

/** Éditeur des règles d'un filtre intelligent. */
export function RulesEditor({ value, onChange }: { value: Rules; onChange: (r: Rules) => void }) {
  const { t } = useTranslation();
  const set = (i: number, patch: Partial<Rule>) => onChange({ ...value, rules: value.rules.map((r, k) => (k === i ? { ...r, ...patch } : r)) });
  return (
    <div className="rules">
      <label className="rules-match">
        {t("playlists.match")}
        <select className="text-input" value={value.matchAll ? "all" : "any"} onChange={(e) => onChange({ ...value, matchAll: e.target.value === "all" })}>
          <option value="all">{t("playlists.matchAll")}</option>
          <option value="any">{t("playlists.matchAny")}</option>
        </select>
      </label>
      {value.rules.map((r, i) => {
        const kind = RULE_FIELDS[r.field] ?? "text";
        const needsValue = !["empty", "not_empty", "is_true", "is_false"].includes(r.op);
        return (
          <div key={i} className="rule-row">
            <select
              className="text-input"
              aria-label={t("playlists.field")}
              value={r.field}
              onChange={(e) => {
                const k = RULE_FIELDS[e.target.value];
                set(i, { field: e.target.value, op: RULE_OPS[k][0], value: "" });
              }}
            >
              {Object.keys(RULE_FIELDS).map((f) => (
                <option key={f} value={f}>{t(`playlists.fields.${f}`)}</option>
              ))}
            </select>
            <select className="text-input" aria-label={t("playlists.op")} value={r.op} onChange={(e) => set(i, { op: e.target.value })}>
              {RULE_OPS[kind].map((o) => (
                <option key={o} value={o}>{t(`playlists.ops.${o}`)}</option>
              ))}
            </select>
            {needsValue ? (
              <input
                className="text-input"
                aria-label={t("playlists.value")}
                inputMode={kind === "number" ? "decimal" : undefined}
                value={r.value}
                onChange={(e) => set(i, { value: e.target.value })}
              />
            ) : (
              <span />
            )}
            <button className="icon-btn" aria-label={t("playlists.removeRule")} onClick={() => onChange({ ...value, rules: value.rules.filter((_, k) => k !== i) })}>
              <X size={16} aria-hidden />
            </button>
          </div>
        );
      })}
      <button className="btn btn-small" onClick={() => onChange({ ...value, rules: [...value.rules, { field: "genre", op: "contains", value: "" }] })}>
        <Plus size={14} aria-hidden /> {t("playlists.addRule")}
      </button>
    </div>
  );
}

function SmartDialog({ initial, onClose, onSave }: { initial?: Playlist; onClose: () => void; onSave: (name: string, rules: Rules) => Promise<void> }) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial?.name ?? "");
  const [rules, setRules] = useState<Rules>(initial?.rules ?? { matchAll: true, rules: [{ field: "genre", op: "contains", value: "" }] });
  const [error, setError] = useState<string | null>(null);
  return (
    <Modal
      wide
      title={initial ? t("playlists.editSmart") : t("playlists.newSmart")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={!name.trim()}
            onClick={async () => {
              try {
                await onSave(name.trim(), rules);
                onClose();
              } catch (e) {
                setError(String((e as { detail?: string })?.detail ?? e));
              }
            }}
          >
            {t("common.ok")}
          </button>
        </>
      }
    >
      <label className="form-label" htmlFor="smart-name">{t("playlists.name")}</label>
      <input id="smart-name" className="text-input full" value={name} onChange={(e) => setName(e.target.value)} />
      <h3 className="form-label">{t("playlists.rules")}</h3>
      <RulesEditor value={rules} onChange={setRules} />
      {error && <p className="text-error small">{t("playlists.invalidRules")} ({error})</p>}
    </Modal>
  );
}

/** Export XML vers Rekordbox : morceaux et playlists choisis. */
export function XmlExportDialog({ selection, onClose, onToast }: { selection: number[]; onClose: () => void; onToast: Props["onToast"] }) {
  const { t } = useTranslation();
  const [lists, setLists] = useState<Playlist[]>([]);
  const [chosen, setChosen] = useState<Set<number>>(new Set());
  const [scope, setScope] = useState<"all" | "selection" | "playlists">(selection.length ? "selection" : "all");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void playlistsList().then((l) => {
      setLists(l);
      setChosen(new Set(l.map((p) => p.id)));
    });
  }, []);
  const run = async () => {
    const path = await pickSave(t("rekordbox.exportXml"), `dj-library-manager-rekordbox-${today()}.xml`, "xml", "XML");
    if (!path) return;
    setBusy(true);
    try {
      const ids = scope === "all" ? null : scope === "selection" ? selection : [];
      const [n, skipped] = await rekordboxExportXml(ids, [...chosen], path);
      onToast(t("rekordbox.exported", { count: n }) + (skipped ? ` ${t("rekordbox.skippedOffline", { count: skipped })}` : ""));
      onClose();
    } catch {
      onToast(t("errors.internal"), undefined, "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      wide
      title={t("rekordbox.exportXml")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || (scope === "playlists" && chosen.size === 0)} onClick={() => void run()}>
            <FileDown size={16} aria-hidden /> {t("rekordbox.exportXmlButton")}
          </button>
        </>
      }
    >
      <p className="hint small">{t("rekordbox.exportXmlHint")}</p>
      <fieldset className="radio-row">
        <legend>{t("rekordbox.exportTracks")}</legend>
        <label><input type="radio" checked={scope === "all"} onChange={() => setScope("all")} /> {t("rekordbox.scopeAll")}</label>
        <label>
          <input type="radio" disabled={!selection.length} checked={scope === "selection"} onChange={() => setScope("selection")} /> {t("rekordbox.scopeSelection", { count: selection.length })}
        </label>
        <label><input type="radio" checked={scope === "playlists"} onChange={() => setScope("playlists")} /> {t("rekordbox.scopePlaylists")}</label>
      </fieldset>
      {lists.length > 0 && (
        <>
          <h3 className="form-label">{t("rekordbox.exportPlaylists")}</h3>
          <div className="checks">
            {lists.map((p) => (
              <label key={p.id}>
                <input
                  type="checkbox"
                  checked={chosen.has(p.id)}
                  onChange={(e) => {
                    const s = new Set(chosen);
                    if (e.target.checked) s.add(p.id);
                    else s.delete(p.id);
                    setChosen(s);
                  }}
                />
                {p.name} <span className="muted small">({p.count})</span>
              </label>
            ))}
          </div>
        </>
      )}
      <p className="hint small">{t("rekordbox.backupHint")}</p>
    </Modal>
  );
}

/** Écran « Playlists et filtres ». */
export function PlaylistsView({ onOpen, onToast, selection }: Props) {
  const { t } = useTranslation();
  const [lists, setLists] = useState<Playlist[] | null>(null);
  const [naming, setNaming] = useState<{ initial: string; submit: (n: string) => Promise<void>; title: string } | null>(null);
  const [smart, setSmart] = useState<{ initial?: Playlist } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Playlist | null>(null);
  const [xml, setXml] = useState(false);

  const load = useCallback(async () => setLists(await playlistsList()), []);
  useEffect(() => {
    void load();
    return onEvent("library:changed", () => void load());
  }, [load]);

  const exportList = async (p: Playlist) => {
    const path = await pickSave(t("playlists.exportM3u"), `${safeName(p.name)}.m3u8`, "m3u8", "M3U8");
    if (!path) return;
    const [n, skipped] = await exportM3u(await playlistTrackIds(p.id), path);
    onToast(t("playlists.exported", { count: n }) + (skipped ? ` ${t("rekordbox.skippedOffline", { count: skipped })}` : ""));
  };

  const row = (p: Playlist) => (
    <li key={p.id} className="playlist-row">
      <button className="playlist-open" onClick={() => onOpen(p)}>
        {p.kind === "smart" ? <Sparkles size={16} aria-hidden /> : <ListMusic size={16} aria-hidden />}
        <span className="playlist-name">{p.name}</span>
        <span className="muted small tabular">{t("playlists.count", { count: p.count })}</span>
      </button>
      <span className="playlist-actions">
        {p.kind === "smart" ? (
          <button className="icon-btn" title={t("playlists.editSmart")} aria-label={t("playlists.editSmart")} onClick={() => setSmart({ initial: p })}>
            <Pencil size={16} aria-hidden />
          </button>
        ) : (
          <button
            className="icon-btn"
            title={t("playlists.rename")}
            aria-label={t("playlists.rename")}
            onClick={() => setNaming({ title: t("playlists.rename"), initial: p.name, submit: async (n) => void (await playlistUpdate(p.id, n, null), await load()) })}
          >
            <Pencil size={16} aria-hidden />
          </button>
        )}
        <button className="icon-btn" title={t("playlists.exportM3u")} aria-label={t("playlists.exportM3u")} onClick={() => void exportList(p)}>
          <FileDown size={16} aria-hidden />
        </button>
        <button className="icon-btn" title={t("playlists.delete")} aria-label={t("playlists.delete")} onClick={() => setConfirmDelete(p)}>
          <Trash2 size={16} aria-hidden />
        </button>
      </span>
    </li>
  );

  const statics = lists?.filter((p) => p.kind === "static") ?? [];
  const smarts = lists?.filter((p) => p.kind === "smart") ?? [];

  return (
    <div className="settings playlists">
      <h1>{t("nav.playlists")}</h1>
      <p className="hint">{t("playlists.intro")}</p>
      <div className="folder-actions">
        <button
          className="btn"
          onClick={() =>
            setNaming({
              title: t("playlists.new"),
              initial: "",
              submit: async (n) => {
                await playlistCreate(n, null);
                await load();
              },
            })
          }
        >
          <Plus size={16} aria-hidden /> {t("playlists.new")}
        </button>
        <button className="btn" onClick={() => setSmart({})}>
          <Sparkles size={16} aria-hidden /> {t("playlists.newSmart")}
        </button>
        <span className="spacer" />
        <button className="btn" onClick={() => setXml(true)}>
          <Upload size={16} aria-hidden /> {t("rekordbox.exportXml")}
        </button>
      </div>

      <section>
        <h2>{t("playlists.static")}</h2>
        {statics.length ? <ul className="playlist-list">{statics.map(row)}</ul> : <p className="muted">{t("playlists.noneStatic")}</p>}
      </section>
      <section>
        <h2>{t("playlists.smart")}</h2>
        {smarts.length ? <ul className="playlist-list">{smarts.map(row)}</ul> : <p className="muted">{t("playlists.noneSmart")}</p>}
      </section>

      {naming && <NameDialog title={naming.title} initial={naming.initial} onClose={() => setNaming(null)} onSubmit={naming.submit} />}
      {smart && (
        <SmartDialog
          initial={smart.initial}
          onClose={() => setSmart(null)}
          onSave={async (name, rules) => {
            if (smart.initial) await playlistUpdate(smart.initial.id, name, rules);
            else await playlistCreate(name, rules);
            await load();
          }}
        />
      )}
      {confirmDelete && (
        <Modal
          title={t("playlists.deleteTitle", { name: confirmDelete.name })}
          onClose={() => setConfirmDelete(null)}
          footer={
            <>
              <button className="btn" onClick={() => setConfirmDelete(null)}>{t("common.cancel")}</button>
              <button
                className="btn btn-danger"
                onClick={async () => {
                  await playlistDelete(confirmDelete.id);
                  setConfirmDelete(null);
                  await load();
                }}
              >
                {t("playlists.delete")}
              </button>
            </>
          }
        >
          <p>{t("playlists.deleteText")}</p>
        </Modal>
      )}
      {xml && <XmlExportDialog selection={selection} onClose={() => setXml(false)} onToast={onToast} />}
    </div>
  );
}
