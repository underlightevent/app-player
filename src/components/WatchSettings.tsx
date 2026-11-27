import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ClipboardCopy, ExternalLink, Link2, Plus, Star, Unlink } from "lucide-react";
import { isCoreError } from "../core/api";
import { DEFAULT_LABELS, OPEN_PLATFORMS, readList, spotifyConnect, spotifyDisconnect, spotifyStatus, watchOpenUrl, type SpotifyStatus } from "../core/watch";

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
}

const SOURCES = ["apple", "deezer", "musicbrainz"] as const;

/** Réglages › Veille musicale : sources gratuites et compte Spotify lié (phase 5). */
export function WatchSettings({ values, onSettingChange, onToast }: Props) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<SpotifyStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [connecting, setConnecting] = useState(false);

  useEffect(() => {
    void spotifyStatus()
      .then((s) => {
        setStatus(s);
        setClientId(s.clientId);
      })
      .catch(() => {});
  }, []);

  const errorText = (e: unknown) =>
    isCoreError(e) ? t(`watch.errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal");

  const saveClientId = (v: string) => {
    setClientId(v);
    onSettingChange("spotify_client_id", v.trim());
  };

  const connect = async () => {
    setConnecting(true);
    try {
      const user = await spotifyConnect();
      setStatus((s) => (s ? { ...s, user } : s));
      onToast(t("watch.spotify.linked", { user }));
    } catch (e) {
      onToast(errorText(e), undefined, "error");
    } finally {
      setConnecting(false);
    }
  };

  const disconnect = async () => {
    try {
      await spotifyDisconnect();
      setStatus((s) => (s ? { ...s, user: "" } : s));
      onToast(t("watch.spotify.unlinked"));
    } catch (e) {
      onToast(errorText(e), undefined, "error");
    }
  };

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      onToast(t("watch.spotify.copied"));
    } catch {
      onToast(text);
    }
  };

  const redirect = status?.redirect ?? "http://127.0.0.1:43821/callback";

  // Labels : liste par défaut + labels ajoutés ; favoris (étoile).
  const favs = readList(values.watch_labels_fav);
  const custom = readList(values.watch_labels_custom);
  const [newLabel, setNewLabel] = useState("");
  const allLabels = [...new Set([...DEFAULT_LABELS, ...custom])].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
  const toggleFav = (l: string) => {
    const on = favs.some((x) => x.toLowerCase() === l.toLowerCase());
    onSettingChange("watch_labels_fav", JSON.stringify(on ? favs.filter((x) => x.toLowerCase() !== l.toLowerCase()) : [...favs, l]));
  };
  const addCustom = () => {
    const l = newLabel.trim();
    if (!l) return;
    const exists = allLabels.find((x) => x.toLowerCase() === l.toLowerCase());
    if (!exists) onSettingChange("watch_labels_custom", JSON.stringify([...custom, l]));
    // Un label ajouté devient favori (c'est la raison habituelle de l'ajouter).
    if (!favs.some((x) => x.toLowerCase() === l.toLowerCase())) onSettingChange("watch_labels_fav", JSON.stringify([...favs, exists ?? l]));
    setNewLabel("");
  };
  const removeCustom = (l: string) => {
    onSettingChange("watch_labels_custom", JSON.stringify(custom.filter((x) => x !== l)));
    onSettingChange("watch_labels_fav", JSON.stringify(favs.filter((x) => x !== l)));
  };

  return (
    <section>
      <h2>{t("watch.settings.title")}</h2>
      <p className="hint">{t("watch.settings.hint")}</p>
      <div className="checks">
        {SOURCES.map((s) => (
          <label key={s}>
            <input type="checkbox" checked={values[`watch_${s}`] === "true"} onChange={(e) => onSettingChange(`watch_${s}`, String(e.target.checked))} />
            <span>
              <strong>{t(`watch.sources.${s}`)}</strong> — {t(`watch.settings.${s}`)}
            </span>
          </label>
        ))}
      </div>

      <h3 className="form-label">{t("watch.settings.previewTitle")}</h3>
      <p className="hint">{t("watch.settings.previewHint")}</p>
      <label className="chip-select">
        {t("watch.settings.previewLabel")}
        <select value={values.watch_preview || "deezer"} onChange={(e) => onSettingChange("watch_preview", e.target.value)}>
          <option value="deezer">Deezer</option>
          <option value="apple">Apple Music</option>
          <option value="off">{t("watch.settings.previewOff")}</option>
        </select>
      </label>

      <h3 className="form-label">{t("watch.settings.openTitle")}</h3>
      <p className="hint">{t("watch.settings.openHint")}</p>
      <label className="chip-select">
        {t("watch.settings.openLabel")}
        <select value={values.watch_open_with || "source"} onChange={(e) => onSettingChange("watch_open_with", e.target.value)}>
          {OPEN_PLATFORMS.map((p) => (
            <option key={p} value={p}>{t(`watch.sites.${p}`)}</option>
          ))}
        </select>
      </label>

      <h3 className="form-label">{t("watch.labels.title")}</h3>
      <p className="hint">{t("watch.labels.hint")}</p>
      <div className="label-list" role="group" aria-label={t("watch.labels.title")}>
        {allLabels.map((l) => {
          const fav = favs.some((x) => x.toLowerCase() === l.toLowerCase());
          const mine = custom.includes(l);
          return (
            <span key={l} className={`label-chip${fav ? " fav" : ""}`}>
              <button className="label-star" aria-pressed={fav} title={fav ? t("watch.labels.unfav") : t("watch.labels.fav")} onClick={() => toggleFav(l)}>
                <Star size={13} fill={fav ? "currentColor" : "none"} aria-hidden /> {l}
              </button>
              {mine && (
                <button className="label-remove" aria-label={t("watch.labels.remove", { label: l })} title={t("watch.labels.remove", { label: l })} onClick={() => removeCustom(l)}>
                  ×
                </button>
              )}
            </span>
          );
        })}
      </div>
      <form
        className="folder-actions"
        onSubmit={(e) => {
          e.preventDefault();
          addCustom();
        }}
      >
        <input className="text-input" value={newLabel} placeholder={t("watch.labels.addPlaceholder")} aria-label={t("watch.labels.addPlaceholder")} onChange={(e) => setNewLabel(e.target.value)} />
        <button className="btn btn-small" type="submit" disabled={!newLabel.trim()}>
          <Plus size={14} aria-hidden /> {t("watch.labels.add")}
        </button>
      </form>

      <h3 className="form-label">{t("watch.settings.spotify")}</h3>
      <p className="hint">{t("watch.settings.spotifyHint")}</p>
      {status?.user ? (
        <div className="folder-actions">
          <span>{t("watch.settings.linkedAs", { user: status.user })}</span>
          <button className="btn btn-small" onClick={() => void disconnect()}>
            <Unlink size={14} aria-hidden /> {t("watch.settings.disconnect")}
          </button>
        </div>
      ) : (
        <>
          <ol className="steps small">
            <li>
              {t("watch.settings.step1")}{" "}
              <button className="link-btn" onClick={() => void watchOpenUrl("https://developer.spotify.com/dashboard")}>
                developer.spotify.com <ExternalLink size={12} aria-hidden />
              </button>
            </li>
            <li>
              {t("watch.settings.step2")} <code>{redirect}</code>{" "}
              <button className="icon-btn" aria-label={t("watch.spotify.copy")} title={t("watch.spotify.copy")} onClick={() => void copy(redirect)}>
                <ClipboardCopy size={14} aria-hidden />
              </button>
            </li>
            <li>{t("watch.settings.step3")}</li>
            <li>{t("watch.settings.step4")}</li>
          </ol>
          <div className="folder-actions">
            <input
              className="text-input"
              value={clientId}
              placeholder={t("watch.settings.clientId")}
              aria-label={t("watch.settings.clientId")}
              spellCheck={false}
              onChange={(e) => saveClientId(e.target.value)}
            />
            <button className="btn btn-primary btn-small" disabled={!clientId.trim() || connecting} onClick={() => void connect()}>
              <Link2 size={14} aria-hidden /> {connecting ? t("watch.settings.connecting") : t("watch.settings.connect")}
            </button>
          </div>
          {connecting && <p className="muted small">{t("watch.settings.connectingHint")}</p>}
        </>
      )}
      <p className="muted small">{t("watch.settings.others")}</p>
    </section>
  );
}
