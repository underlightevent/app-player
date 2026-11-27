import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Modal } from "./Modal";
import { spotifyAdd, spotifyCreatePlaylist, spotifyFind, spotifyPlaylists, type SpotifyMatch, type SpotifyPlaylist, type WishItem } from "../core/watch";

interface Props {
  /** Envies à ajouter. */
  wishes: WishItem[];
  onClose: () => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  errorText: (e: unknown) => string;
}

const NEW = "__new__";

/** Ajout d'envies à une playlist du compte Spotify lié : aperçu des correspondances, puis ajout. */
export function SpotifyAddDialog({ wishes, onClose, onToast, errorText }: Props) {
  const { t } = useTranslation();
  const ids = wishes.map((w) => w.id);
  const wishOf = (id: number) => wishes.find((w) => w.id === id);
  const [matches, setMatches] = useState<SpotifyMatch[] | null>(null);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [playlists, setPlaylists] = useState<SpotifyPlaylist[] | null>(null);
  const [target, setTarget] = useState<string>(NEW);
  const [newName, setNewName] = useState(t("watch.spotify.defaultName"));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [m, p] = await Promise.all([spotifyFind(ids), spotifyPlaylists()]);
        if (!alive) return;
        setMatches(m);
        setChecked(new Set(m.filter((x) => x.uri).map((x) => x.wishId)));
        setPlaylists(p);
        if (p.length) setTarget(p[0].id);
      } catch (e) {
        if (alive) setError(errorText(e));
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const uris = (matches ?? []).filter((m) => m.uri && checked.has(m.wishId)).map((m) => m.uri!);
  const found = (matches ?? []).filter((m) => m.uri).length;

  const confirm = async () => {
    setBusy(true);
    try {
      let id = target;
      let name = playlists?.find((p) => p.id === target)?.name ?? "";
      if (target === NEW) {
        const p = await spotifyCreatePlaylist(newName);
        id = p.id;
        name = p.name;
      }
      const n = await spotifyAdd(id, uris);
      onToast(t("watch.spotify.done", { count: n, name }));
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("watch.spotify.title")}
      wide
      onClose={() => !busy && onClose()}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>{t("common.cancel")}</button>
          <button
            className="btn btn-primary"
            disabled={busy || !matches || uris.length === 0 || (target === NEW && !newName.trim())}
            onClick={() => void confirm()}
          >
            {t("watch.spotify.confirm", { count: uris.length })}
          </button>
        </>
      }
    >
      {error && <p className="note-info text-error" role="alert">{error}</p>}
      {!matches && !error && <p className="muted">{t("watch.spotify.searching", { count: ids.length })}</p>}
      {matches && (
        <>
          <p>{t("watch.spotify.summary", { found, total: matches.length })}</p>
          <div className="form-row">
            <label className="form-label" htmlFor="sp-target">{t("watch.spotify.playlist")}</label>
            <select id="sp-target" value={target} onChange={(e) => setTarget(e.target.value)}>
              {(playlists ?? []).map((p) => (
                <option key={p.id} value={p.id}>{p.name} ({p.tracks})</option>
              ))}
              <option value={NEW}>{t("watch.spotify.newPlaylist")}</option>
            </select>
            {target === NEW && (
              <input className="text-input" value={newName} aria-label={t("watch.spotify.newName")} onChange={(e) => setNewName(e.target.value)} />
            )}
          </div>
          <table className="plain-table watch-table">
            <thead>
              <tr>
                <th />
                <th>{t("watch.spotify.wanted")}</th>
                <th>{t("watch.spotify.foundOn")}</th>
              </tr>
            </thead>
            <tbody>
              {matches.map((m) => (
                <tr key={m.wishId} className={m.uri ? "" : "muted"}>
                  <td>
                    <input
                      type="checkbox"
                      disabled={!m.uri}
                      aria-label={m.name}
                      checked={checked.has(m.wishId)}
                      onChange={(e) => {
                        const s = new Set(checked);
                        if (e.target.checked) s.add(m.wishId);
                        else s.delete(m.wishId);
                        setChecked(s);
                      }}
                    />
                  </td>
                  <td>
                    <div>{wishOf(m.wishId)?.title}{wishOf(m.wishId)?.version ? ` (${wishOf(m.wishId)?.version})` : ""}</div>
                    <div className="muted small">{wishOf(m.wishId)?.artists.join(", ")}</div>
                  </td>
                  <td>
                    {m.uri ? (
                      <>
                        <div>{m.name}</div>
                        <div className="muted small">{m.artists.join(", ")}{m.album ? ` · ${m.album}` : ""}</div>
                      </>
                    ) : (
                      <div className="muted small">{t("watch.spotify.notFound")}</div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="muted small">{t("watch.spotify.checkHint")}</p>
        </>
      )}
    </Modal>
  );
}
