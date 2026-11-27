import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ExternalLink, EyeOff, FileDown, Heart, Info, ListPlus, LoaderCircle, Music2, Pause, Play, RefreshCw, Search, Trash2, Star, WifiOff } from "lucide-react";
import { isCoreError } from "../core/api";
import { onEvent } from "../core/library";
import { pickSave } from "../core/rekordbox";
import {
  COUNTRIES,
  DEFAULT_LABELS,
  readList,
  openTarget,
  shopLinks,
  spotifyStatus,
  watchChart,
  watchGenres,
  watchOpenUrl,
  watchReleases,
  wishlistAdd,
  wishlistExportCsv,
  wishlistList,
  wishlistRemove,
  wishlistSetIgnored,
  type Genre,
  type SpotifyStatus,
  type WatchItem,
  type WishItem,
} from "../core/watch";
import { SpotifyAddDialog } from "./SpotifyAddDialog";

type Tab = "charts" | "releases" | "wishlist";

interface Props {
  values: Record<string, string>;
  onSettingChange: (key: string, value: string) => void;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  /** Ouvre Réglages › Veille musicale. */
  onOpenSettings: () => void;
  /** Le nombre d'envies a changé (badge de la barre latérale). */
  onWishlistChanged: () => void;
  /** Extrait en cours dans le lecteur du bas (clé de la ligne), et son état. */
  playingKey: string | null;
  playing: boolean;
  /** Lance l'extrait `index` de la liste dans le lecteur du bas. */
  onPlayPreview: (items: WatchItem[], keys: string[], index: number) => Promise<boolean>;
  onTogglePlay: () => void;
}

interface Last {
  tab: Tab;
  chartSource: "apple" | "deezer";
  country: string;
  genre: number;
  kind: "songs" | "albums";
  relMode: "label" | "country";
  /** Label choisi, « __fav__ » (tous les favoris) ou « __other__ » (saisi à la main). */
  label: string;
  labelOther: string;
  relCountry: string;
  days: number;
}

const DEFAULT_LAST: Last = {
  tab: "charts",
  chartSource: "apple",
  country: "fr",
  genre: 0,
  kind: "songs",
  relMode: "label",
  label: "",
  labelOther: "",
  relCountry: "fr",
  days: 30,
};

function readLast(raw: string | undefined): Last {
  try {
    return { ...DEFAULT_LAST, ...(raw ? (JSON.parse(raw) as Partial<Last>) : {}) };
  } catch {
    return DEFAULT_LAST;
  }
}

/** Écran Veille : classements, nouvelles sorties et liste d'envies (phase 5). */
export function WatchView({ values, onSettingChange, onToast, onOpenSettings, onWishlistChanged, playingKey, playing, onPlayPreview, onTogglePlay }: Props) {
  const { t, i18n } = useTranslation();
  const [last, setLastState] = useState<Last>(() => readLast(values.watch_last));
  const setLast = (patch: Partial<Last>) =>
    setLastState((l) => {
      const n = { ...l, ...patch };
      onSettingChange("watch_last", JSON.stringify(n));
      return n;
    });

  const enabled = {
    apple: values.watch_apple === "true",
    deezer: values.watch_deezer === "true",
    musicbrainz: values.watch_musicbrainz === "true",
  };
  const anyEnabled = enabled.apple || enabled.deezer || enabled.musicbrainz;

  const [items, setItems] = useState<WatchItem[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [genres, setGenres] = useState<Genre[]>([]);
  const [wish, setWish] = useState<WishItem[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [spotify, setSpotify] = useState<SpotifyStatus | null>(null);
  const [spotifyWishes, setSpotifyWishes] = useState<WishItem[] | null>(null);
  const [filter, setFilter] = useState<"all" | "wanted" | "found" | "ignored">("wanted");

  const regionName = useMemo(() => {
    try {
      const dn = new Intl.DisplayNames([i18n.language], { type: "region" });
      return (c: string) => dn.of(c.toUpperCase()) ?? c.toUpperCase();
    } catch {
      return (c: string) => c.toUpperCase();
    }
  }, [i18n.language]);
  const countries = useMemo(() => [...COUNTRIES].sort((a, b) => regionName(a).localeCompare(regionName(b), i18n.language)), [regionName, i18n.language]);

  const errorText = (e: unknown) =>
    isCoreError(e) ? t(`watch.errors.${e.code}`, { defaultValue: t(`errors.${e.code}`, { defaultValue: e.detail }) }) : t("errors.internal");

  const chartSource = enabled[last.chartSource] ? last.chartSource : enabled.apple ? "apple" : "deezer";

  const loadWish = useCallback(async () => {
    setWish(await wishlistList().catch(() => []));
  }, []);
  useEffect(() => {
    void loadWish();
    void spotifyStatus().then(setSpotify).catch(() => setSpotify(null));
    return onEvent("library:changed", () => void loadWish());
  }, [loadWish]);

  useEffect(() => {
    if (enabled.deezer && genres.length === 0) void watchGenres().then(setGenres).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled.deezer]);

  const run = async (fn: () => Promise<WatchItem[]>) => {
    setLoading(true);
    setError(null);
    try {
      setItems(await fn());
    } catch (e) {
      setItems(null);
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  };

  const loadChart = () => void run(() => watchChart(chartSource, last.country, last.genre, last.kind, 100));
  // Labels : grands labels par défaut + ceux ajoutés dans les réglages ; favoris en tête.
  const favs = readList(values.watch_labels_fav);
  const custom = readList(values.watch_labels_custom);
  const allLabels = useMemo(
    () => [...new Set([...DEFAULT_LABELS, ...custom])].sort((a, b) => a.localeCompare(b, i18n.language, { sensitivity: "base" })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [values.watch_labels_custom, i18n.language],
  );
  const labelChoice = last.label || (favs.length ? "__fav__" : "Spinnin' Records");
  const chosenLabels = (): string[] =>
    labelChoice === "__fav__" ? favs : labelChoice === "__other__" ? [last.labelOther.trim()].filter(Boolean) : [labelChoice];
  const isFav = (l: string) => favs.some((x) => x.toLowerCase() === l.toLowerCase());
  const toggleFav = (l: string) =>
    onSettingChange("watch_labels_fav", JSON.stringify(isFav(l) ? favs.filter((x) => x.toLowerCase() !== l.toLowerCase()) : [...favs, l]));
  const loadReleases = () =>
    void run(() => watchReleases(last.relMode === "label" ? chosenLabels() : [], last.relMode === "country" ? last.relCountry : null, last.days));

  // Classement chargé à l'ouverture et à chaque changement de pays, genre ou type.
  useEffect(() => {
    if (last.tab === "charts" && (enabled.apple || enabled.deezer)) loadChart();
    if (last.tab === "releases") setItems(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [last.tab, chartSource, last.country, last.genre, last.kind, enabled.apple, enabled.deezer]);

  const add = async (list: WatchItem[]) => {
    try {
      const n = await wishlistAdd(list);
      onToast(n > 0 ? t("watch.added", { count: n }) : t("watch.alreadyThere"));
      const keys = new Set(list.map((i) => `${i.artists[0] ?? ""}|${i.title}`));
      setItems((cur) => cur?.map((i) => (keys.has(`${i.artists[0] ?? ""}|${i.title}`) ? { ...i, inWishlist: true } : i)) ?? cur);
      await loadWish();
      onWishlistChanged();
    } catch (e) {
      onToast(errorText(e), undefined, "error");
    }
  };

  const remove = async (ids: number[]) => {
    await wishlistRemove(ids, true);
    setSelected(new Set());
    await loadWish();
    onWishlistChanged();
    onToast(t("watch.removed", { count: ids.length }), {
      label: t("common.undo"),
      run: () =>
        void wishlistRemove(ids, false).then(async () => {
          await loadWish();
          onWishlistChanged();
        }),
    });
  };

  const setIgnored = async (ids: number[], ignored: boolean) => {
    await wishlistSetIgnored(ids, ignored);
    setSelected(new Set());
    await loadWish();
    onWishlistChanged();
  };

  const exportCsv = async () => {
    const p = await pickSave(t("watch.exportCsv"), `liste-d-envies-${new Date().toISOString().slice(0, 10)}.csv`, "csv", "CSV");
    if (!p) return;
    try {
      await wishlistExportCsv(p);
      onToast(t("watch.exported"));
    } catch (e) {
      onToast(errorText(e), undefined, "error");
    }
  };

  const open = (url: string) => void watchOpenUrl(url).catch((e) => onToast(errorText(e), undefined, "error"));

  // ---------- Écoute des extraits : lecteur du bas de l'application ----------
  const previewOff = values.watch_preview === "off";
  const [loadingKey, setLoadingKey] = useState<string | null>(null);

  /** ▶ sur une pochette : lance l'extrait (ou pause / reprend si c'est celui en cours). */
  const play = async (list: WatchItem[], keys: string[], index: number) => {
    const key = keys[index];
    if (playingKey === key) {
      onTogglePlay();
      return;
    }
    setLoadingKey(key);
    try {
      await onPlayPreview(list, keys, index);
    } finally {
      setLoadingKey(null);
    }
  };

  const coverCell = (item: WatchItem, list: WatchItem[], keys: string[], index: number) => {
    const key = keys[index];
    const mine = playingKey === key;
    const icon =
      loadingKey === key ? <LoaderCircle size={18} className="spin" aria-hidden /> : mine && playing ? <Pause size={18} aria-hidden /> : <Play size={18} aria-hidden />;
    const img = item.coverUrl ? (
      <img className="watch-cover" src={item.coverUrl} alt="" loading="lazy" onError={(e) => (e.currentTarget.style.visibility = "hidden")} />
    ) : (
      <span className="watch-cover placeholder"><Music2 size={16} aria-hidden /></span>
    );
    return (
      <td className="watch-cover-cell">
        {previewOff ? (
          img
        ) : (
          <button
            className={`watch-cover-play${mine ? " active" : ""}${loadingKey === key ? " loading" : ""}`}
            title={mine && playing ? t("player.pause") : t("watch.preview.play")}
            aria-label={`${mine && playing ? t("player.pause") : t("watch.preview.play")} : ${item.title}`}
            onClick={() => void play(list, keys, index)}
          >
            {img}
            <span className="watch-cover-icon">{icon}</span>
          </button>
        )}
      </td>
    );
  };

  const openWith = values.watch_open_with || "source";
  const siteLabel = (s: string) => t(`watch.sites.${s}`, { defaultValue: s });
  const rowButtons = (item: WatchItem) => {
    const target = openTarget(item, openWith);
    return (
      target && (
        <button
          className="icon-btn"
          title={t("watch.openSource", { source: siteLabel(target.site) })}
          aria-label={t("watch.openSource", { source: siteLabel(target.site) })}
          onClick={() => open(target.url)}
        >
          <ExternalLink size={16} aria-hidden />
        </button>
      )
    );
  };

  const shown = (wish ?? []).filter((w) => filter === "all" || w.status === filter);
  const shownKeys = shown.map((w) => `w-${w.id}`);
  const counts = {
    all: wish?.length ?? 0,
    wanted: wish?.filter((w) => w.status === "wanted").length ?? 0,
    found: wish?.filter((w) => w.status === "found").length ?? 0,
    ignored: wish?.filter((w) => w.status === "ignored").length ?? 0,
  };
  const date = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" });
  const fmtDate = (d: string | null) => {
    if (!d) return "";
    const parsed = new Date(d.length === 4 ? `${d}-01-01` : d);
    return d.length === 4 || Number.isNaN(parsed.getTime()) ? d : date.format(parsed);
  };

  const tabs: { id: Tab; label: string }[] = [
    { id: "charts", label: t("watch.tabs.charts") },
    { id: "releases", label: t("watch.tabs.releases") },
    { id: "wishlist", label: `${t("watch.tabs.wishlist")}${counts.wanted ? ` (${counts.wanted})` : ""}` },
  ];

  const sourceLabel = (s: string) => t(`watch.sources.${s}`, { defaultValue: s });

  const resultRows = (list: WatchItem[]) => {
    const keys = list.map((it, i) => `${last.tab}-${i}-${it.artists[0] ?? ""}-${it.title}`);
    return (
    <table className="plain-table watch-table">
      <thead>
        <tr>
          {last.tab === "charts" && <th className="num">#</th>}
          <th />
          <th>{t("watch.col.title")}</th>
          <th>{last.tab === "releases" ? t("watch.col.label") : t("watch.col.kind")}</th>
          <th>{t("watch.col.date")}</th>
          <th />
        </tr>
      </thead>
      <tbody>
        {list.map((it, i) => (
          <tr key={`${it.title}-${i}`}>
            {last.tab === "charts" && <td className="num tabular">{it.rank}</td>}
            {coverCell(it, list, keys, i)}
            <td>
              <div className="watch-title">
                {it.title}
                {it.version && <span className="muted"> ({it.version})</span>}
              </div>
              <div className="muted small">{it.artists.join(", ")}</div>
            </td>
            <td className="muted small">{last.tab === "releases" ? it.label ?? "" : it.kind === "album" ? t("watch.album") : t("watch.track")}</td>
            <td className="muted small tabular">{fmtDate(it.releaseDate)}</td>
            <td className="watch-actions">
              {it.inLibrary && <span className="badge badge-info" title={t("watch.inLibraryHint")}><Check size={12} aria-hidden /> {t("watch.inLibrary")}</span>}
              {it.inWishlist ? (
                <span className="badge" title={t("watch.inWishlistHint")}><Heart size={12} aria-hidden /> {t("watch.inWishlist")}</span>
              ) : (
                <button className="btn btn-small" onClick={() => void add([it])}>
                  <ListPlus size={14} aria-hidden /> {t("watch.add")}
                </button>
              )}
              {rowButtons(it)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
    );
  };

  return (
    <div className="settings watch">
      <h1>{t("nav.watch")}</h1>
      <div className="segmented" role="tablist" aria-label={t("nav.watch")}>
        {tabs.map((tb) => (
          <button key={tb.id} role="tab" aria-selected={last.tab === tb.id} aria-pressed={last.tab === tb.id} onClick={() => setLast({ tab: tb.id })}>
            {tb.label}
          </button>
        ))}
      </div>

      {last.tab !== "wishlist" && !anyEnabled && (
        <div className="empty small-empty">
          <WifiOff size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("watch.noSource")}</p>
          <button className="btn btn-primary" onClick={onOpenSettings}>{t("watch.openSettings")}</button>
        </div>
      )}

      {last.tab === "charts" && anyEnabled && (
        <>
          {!(enabled.apple || enabled.deezer) ? (
            <p className="note-info"><Info size={14} aria-hidden /> {t("watch.chartsDisabled")} <button className="link-btn" onClick={onOpenSettings}>{t("watch.openSettings")}</button></p>
          ) : (
            <div className="folder-actions watch-controls">
              <label className="chip-select">
                {t("watch.source")}
                <select value={chartSource} onChange={(e) => setLast({ chartSource: e.target.value as "apple" | "deezer" })}>
                  {enabled.apple && <option value="apple">{t("watch.sources.apple")}</option>}
                  {enabled.deezer && <option value="deezer">{t("watch.sources.deezer")}</option>}
                </select>
              </label>
              {chartSource === "apple" ? (
                <label className="chip-select">
                  {t("watch.country")}
                  <select value={last.country} onChange={(e) => setLast({ country: e.target.value })}>
                    {countries.map((c) => (
                      <option key={c} value={c}>{regionName(c)}</option>
                    ))}
                  </select>
                </label>
              ) : (
                <label className="chip-select">
                  {t("watch.genre")}
                  <select value={last.genre} onChange={(e) => setLast({ genre: Number(e.target.value) })}>
                    {(genres.length ? genres : [{ id: 0, name: t("watch.allGenres") }]).map((g) => (
                      <option key={g.id} value={g.id}>{g.name}</option>
                    ))}
                  </select>
                </label>
              )}
              <label className="chip-select">
                {t("watch.kind")}
                <select value={last.kind} onChange={(e) => setLast({ kind: e.target.value as "songs" | "albums" })}>
                  <option value="songs">{t("watch.kinds.songs")}</option>
                  <option value="albums">{t("watch.kinds.albums")}</option>
                </select>
              </label>
              <button className="btn btn-small" onClick={loadChart} disabled={loading}>
                <RefreshCw size={14} aria-hidden /> {t("watch.refresh")}
              </button>
              <span className="spacer" />
              {items && items.some((i) => !i.inWishlist) && (
                <button className="btn btn-small" onClick={() => void add(items.filter((i) => !i.inWishlist && !i.inLibrary))}>
                  <ListPlus size={14} aria-hidden /> {t("watch.addAllMissing")}
                </button>
              )}
            </div>
          )}
          {chartSource === "deezer" && (enabled.apple || enabled.deezer) && <p className="muted small">{t("watch.deezerCountryHint")}</p>}
        </>
      )}

      {last.tab === "releases" && anyEnabled && (
        <>
          {!enabled.musicbrainz ? (
            <p className="note-info"><Info size={14} aria-hidden /> {t("watch.releasesDisabled")} <button className="link-btn" onClick={onOpenSettings}>{t("watch.openSettings")}</button></p>
          ) : (
            <form
              className="folder-actions watch-controls"
              onSubmit={(e) => {
                e.preventDefault();
                loadReleases();
              }}
            >
              <div className="segmented" role="group" aria-label={t("watch.releasesBy")}>
                <button type="button" aria-pressed={last.relMode === "label"} onClick={() => setLast({ relMode: "label" })}>{t("watch.byLabel")}</button>
                <button type="button" aria-pressed={last.relMode === "country"} onClick={() => setLast({ relMode: "country" })}>{t("watch.byCountry")}</button>
              </div>
              {last.relMode === "label" ? (
                <>
                  <label className="chip-select">
                    {t("watch.col.label")}
                    <select value={labelChoice} onChange={(e) => setLast({ label: e.target.value })}>
                      <option value="__fav__" disabled={favs.length === 0}>
                        {t("watch.labels.allFavs", { count: favs.length })}
                      </option>
                      {favs.length > 0 && (
                        <optgroup label={t("watch.labels.favs")}>
                          {favs.map((l) => (
                            <option key={`f-${l}`} value={l}>★ {l}</option>
                          ))}
                        </optgroup>
                      )}
                      <optgroup label={t("watch.labels.all")}>
                        {allLabels.filter((l) => !isFav(l)).map((l) => (
                          <option key={l} value={l}>{l}</option>
                        ))}
                      </optgroup>
                      <option value="__other__">{t("watch.labels.other")}</option>
                    </select>
                  </label>
                  {labelChoice === "__other__" && (
                    <input
                      className="text-input"
                      value={last.labelOther}
                      placeholder={t("watch.labelPlaceholder")}
                      aria-label={t("watch.labels.other")}
                      onChange={(e) => setLast({ labelOther: e.target.value })}
                    />
                  )}
                  {labelChoice !== "__fav__" && chosenLabels()[0] && (
                    <button
                      type="button"
                      className={`icon-btn${isFav(chosenLabels()[0]) ? " active" : ""}`}
                      title={isFav(chosenLabels()[0]) ? t("watch.labels.unfav") : t("watch.labels.fav")}
                      aria-label={isFav(chosenLabels()[0]) ? t("watch.labels.unfav") : t("watch.labels.fav")}
                      onClick={() => toggleFav(chosenLabels()[0])}
                    >
                      <Star size={16} fill={isFav(chosenLabels()[0]) ? "currentColor" : "none"} aria-hidden />
                    </button>
                  )}
                </>
              ) : (
                <label className="chip-select">
                  {t("watch.country")}
                  <select value={last.relCountry} onChange={(e) => setLast({ relCountry: e.target.value })}>
                    {countries.map((c) => (
                      <option key={c} value={c}>{regionName(c)}</option>
                    ))}
                  </select>
                </label>
              )}
              <label className="chip-select">
                {t("watch.period")}
                <select value={last.days} onChange={(e) => setLast({ days: Number(e.target.value) })}>
                  {[7, 30, 90].map((d) => (
                    <option key={d} value={d}>{t("watch.days", { count: d })}</option>
                  ))}
                </select>
              </label>
              <button className="btn btn-primary btn-small" type="submit" disabled={loading || (last.relMode === "label" && chosenLabels().length === 0)}>
                <Search size={14} aria-hidden /> {t("watch.search")}
              </button>
            </form>
          )}
        </>
      )}

      {last.tab !== "wishlist" && anyEnabled && (
        <>
          {loading && <p className="muted">{t("watch.loading")}</p>}
          {error && (
            <p className="note-info text-error" role="alert">
              <WifiOff size={14} aria-hidden /> {error}
            </p>
          )}
          {!loading && items && items.length === 0 && <p className="muted">{t("watch.nothing")}</p>}
          {!loading && items && items.length > 0 && resultRows(items)}
          {last.tab === "releases" && enabled.musicbrainz && <p className="muted small">{t("watch.musicbrainzHint")}</p>}
        </>
      )}

      {last.tab === "wishlist" && (
        <>
          <p className="note-info"><Info size={14} aria-hidden /> {t("watch.wishIntro")}</p>
          <div className="folder-actions watch-controls">
            <div className="segmented" role="group" aria-label={t("watch.filter")}>
              {(["wanted", "found", "ignored", "all"] as const).map((f) => (
                <button key={f} aria-pressed={filter === f} onClick={() => setFilter(f)}>
                  {t(`watch.status.${f}`)} ({counts[f]})
                </button>
              ))}
            </div>
            <span className="spacer" />
            {spotify?.user ? (
              <button
                className="btn btn-small"
                disabled={shown.length === 0}
                onClick={() => setSpotifyWishes(selected.size ? (wish ?? []).filter((w) => selected.has(w.id)) : shown.filter((w) => w.status !== "ignored"))}
              >
                <Music2 size={14} aria-hidden /> {selected.size ? t("watch.spotifyAddSelected", { count: selected.size }) : t("watch.spotifyAddAll")}
              </button>
            ) : (
              <button className="btn btn-small" onClick={onOpenSettings} title={t("watch.spotifyLinkHint")}>
                <Music2 size={14} aria-hidden /> {t("watch.spotifyLink")}
              </button>
            )}
            <button className="btn btn-small" disabled={!wish?.length} onClick={() => void exportCsv()}>
              <FileDown size={14} aria-hidden /> {t("watch.exportCsv")}
            </button>
          </div>
          {selected.size > 0 && (
            <div className="folder-actions">
              <span className="muted">{t("watch.selected", { count: selected.size })}</span>
              <button className="btn btn-small" onClick={() => void setIgnored([...selected], true)}><EyeOff size={14} aria-hidden /> {t("watch.ignore")}</button>
              <button className="btn btn-small" onClick={() => void setIgnored([...selected], false)}>{t("watch.unignore")}</button>
              <button className="btn btn-small btn-danger" onClick={() => void remove([...selected])}><Trash2 size={14} aria-hidden /> {t("watch.remove")}</button>
            </div>
          )}
          {wish && shown.length === 0 ? (
            <div className="empty small-empty">
              <Heart size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
              <p>{counts.all === 0 ? t("watch.wishEmpty") : t("watch.wishEmptyFilter")}</p>
            </div>
          ) : (
            <table className="plain-table watch-table">
              <thead>
                <tr>
                  <th>
                    <input
                      type="checkbox"
                      aria-label={t("common.selectAll")}
                      checked={shown.length > 0 && shown.every((w) => selected.has(w.id))}
                      onChange={(e) => setSelected(new Set(e.target.checked ? shown.map((w) => w.id) : []))}
                    />
                  </th>
                  <th />
                  <th>{t("watch.col.title")}</th>
                  <th>{t("watch.col.status")}</th>
                  <th>{t("watch.col.shops")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {shown.map((w) => (
                  <tr key={w.id} className={w.status === "ignored" ? "muted" : ""}>
                    <td>
                      <input
                        type="checkbox"
                        aria-label={w.title}
                        checked={selected.has(w.id)}
                        onChange={(e) => {
                          const s = new Set(selected);
                          if (e.target.checked) s.add(w.id);
                          else s.delete(w.id);
                          setSelected(s);
                        }}
                      />
                    </td>
                    {coverCell(w, shown, shownKeys, shown.indexOf(w))}
                    <td>
                      <div className="watch-title">
                        {w.title}
                        {w.version && <span className="muted"> ({w.version})</span>}
                      </div>
                      <div className="muted small">
                        {w.artists.join(", ")}
                        {w.label ? ` · ${w.label}` : ""}
                        {w.releaseDate ? ` · ${fmtDate(w.releaseDate)}` : ""} · {sourceLabel(w.source)}
                      </div>
                    </td>
                    <td>
                      <span className={`badge ${w.status === "found" ? "badge-info" : ""}`}>
                        {w.status === "found" && <Check size={12} aria-hidden />} {t(`watch.status.${w.status}`)}
                      </span>
                    </td>
                    <td className="watch-shops">
                      {shopLinks(w).map((l) => (
                        <button key={l.shop} className="link-btn" onClick={() => open(l.url)}>{l.shop}</button>
                      ))}
                    </td>
                    <td className="watch-actions">
                      {rowButtons(w)}
                      <button className="icon-btn" title={t("watch.remove")} aria-label={t("watch.remove")} onClick={() => void remove([w.id])}>
                        <Trash2 size={16} aria-hidden />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}

      {spotifyWishes && (
        <SpotifyAddDialog
          wishes={spotifyWishes}
          onClose={() => setSpotifyWishes(null)}
          onToast={onToast}
          errorText={errorText}
        />
      )}
    </div>
  );
}
