import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowRight, ListMusic, Plus, TriangleAlert } from "lucide-react";
import type { EditReport } from "../core/edit";
import { convertApply, convertPreview, playlistAdd, playlistCreate, playlistsList, type ConvertItem, type ConvertTarget, type Playlist } from "../core/rekordbox";
import { Modal } from "./Modal";

/** Conversion sans perte (AIFF ou WAV, 16 / 24 bits) : aperçu, alertes Rekordbox, confirmation. */
export function ConvertDialog({ ids, onClose, onDone }: { ids: number[]; onClose: () => void; onDone: (r: EditReport) => void }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<ConvertItem[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [target, setTarget] = useState<ConvertTarget>({ format: "aiff", bits: null });
  useEffect(() => {
    setItems(null);
    void convertPreview(ids, target).then(setItems).catch(() => setItems([]));
  }, [ids, target]);
  const ok = items?.filter((i) => i.status === "ok") ?? [];
  const inRb = ok.filter((i) => i.inRekordbox).length;
  return (
    <Modal
      wide
      title={t("convert.title", { count: ok.length })}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={busy || ok.length === 0}
            onClick={async () => {
              setBusy(true);
              try {
                onDone(await convertApply(ok.map((i) => i.trackId), target));
                onClose();
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("convert.apply", { count: ok.length })}
          </button>
        </>
      }
    >
      <div className="folder-actions">
        <label className="chip-select">
          {t("convert.format")}
          <select value={target.format} onChange={(e) => setTarget({ ...target, format: e.target.value as ConvertTarget["format"] })}>
            <option value="aiff">AIFF</option>
            <option value="wav">WAV</option>
          </select>
        </label>
        <label className="chip-select">
          {t("convert.depth")}
          <select value={target.bits ?? ""} onChange={(e) => setTarget({ ...target, bits: e.target.value ? (Number(e.target.value) as 16 | 24) : null })}>
            <option value="">{t("convert.keepDepth")}</option>
            <option value="24">24 bits</option>
            <option value="16">16 bits</option>
          </select>
        </label>
      </div>
      <p className="hint">{t("convert.hint")}</p>
      {target.bits === 16 && <p className="hint small text-warning">{t("convert.depthWarning")}</p>}
      {inRb > 0 && (
        <p className="rekordbox-alert" role="alert"><TriangleAlert size={16} aria-hidden /> {t("convert.rekordboxAlert", { count: inRb })}</p>
      )}
      <div className="preview-list" aria-busy={items == null}>
        {items?.map((i) => (
          <div key={i.trackId} className={`preview-item status-${i.status}`}>
            <span className="preview-old">{i.fileName}</span>
            <ArrowRight size={14} aria-hidden />
            <span className="preview-new">
              {i.status === "ok" ? i.targetName : "—"}
              {i.status !== "ok" && (
                <span className="badge badge-error" title={i.detail ?? ""}>{t(`convert.status.${i.status}`)}</span>
              )}
              {i.inRekordbox && i.status === "ok" && <span className="badge badge-rekordbox">Rekordbox</span>}
            </span>
          </div>
        ))}
      </div>
      <p className="hint small">{t("convert.after")}</p>
    </Modal>
  );
}

/** Ajouter des morceaux à une playlist (ou en créer une). */
export function AddToPlaylistDialog({ ids, onClose, onDone }: { ids: number[]; onClose: () => void; onDone: (text: string) => void }) {
  const { t } = useTranslation();
  const [lists, setLists] = useState<Playlist[]>([]);
  const [name, setName] = useState("");
  useEffect(() => {
    // Playlists et sets (phase 6) ; les filtres intelligents se remplissent tout seuls.
    void playlistsList().then((l) => setLists(l.filter((p) => p.kind === "static" || p.kind === "set")));
  }, []);
  const add = async (p: Playlist) => {
    const n = await playlistAdd(p.id, ids);
    onDone(t("playlists.added", { count: n, name: p.name }));
    onClose();
  };
  return (
    <Modal title={t("playlists.addTitle", { count: ids.length })} onClose={onClose}>
      {lists.length > 0 && (
        <ul className="playlist-list">
          {lists.map((p) => (
            <li key={p.id} className="playlist-row">
              <button className="playlist-open" onClick={() => void add(p)}>
                <ListMusic size={16} aria-hidden />
                <span className="playlist-name">{p.kind === "set" ? `${t("sets.prefix")} ${p.name}` : p.name}</span>
                <span className="muted small tabular">{t("playlists.count", { count: p.count })}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="inline-form"
        onSubmit={async (e) => {
          e.preventDefault();
          if (!name.trim()) return;
          await playlistCreate(name.trim(), null, ids);
          onDone(t("playlists.created", { name: name.trim(), count: ids.length }));
          onClose();
        }}
      >
        <input className="text-input" placeholder={t("playlists.newName")} aria-label={t("playlists.newName")} value={name} onChange={(e) => setName(e.target.value)} />
        <button className="btn" type="submit" disabled={!name.trim()}>
          <Plus size={16} aria-hidden /> {t("playlists.createWith")}
        </button>
      </form>
    </Modal>
  );
}
