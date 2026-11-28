import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileDown, Info, Play, RotateCcw, Square, WandSparkles, Waves } from "lucide-react";
import { isCoreError } from "../core/api";
import { libraryRows, type TrackRow } from "../core/library";
import { playerCommand } from "../core/player";
import {
  DEFAULT_SETTINGS,
  PRESETS,
  remasterAb,
  remasterAuto,
  remasterBatch,
  remasterExport,
  remasterGet,
  remasterLevel,
  remasterListen,
  remasterSpectrum,
  remasterUpdate,
  type Prepared,
  type RemasterSettings,
} from "../core/remaster";

interface Props {
  selection: number[];
  /** Morceau en cours dans le lecteur du bas (pour savoir si l'écoute Remaster joue). */
  playingId: number | null;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  onOpenTrack: (id: number) => void;
}

const BAND_LABELS = ["low", "lowMid", "mid", "highMid", "high"];

/** Curseur avec libellé et valeur affichée. */
function Slider(p: { label: string; value: number; min: number; max: number; step: number; unit?: string; disabled?: boolean; onChange: (v: number) => void; format?: (v: number) => string }) {
  return (
    <label className={`rm-slider${p.disabled ? " disabled" : ""}`}>
      <span className="rm-slider-label">{p.label}</span>
      <input type="range" min={p.min} max={p.max} step={p.step} value={p.value} disabled={p.disabled} onChange={(e) => p.onChange(Number(e.target.value))} />
      <span className="rm-slider-value tabular">{p.format ? p.format(p.value) : `${p.value}${p.unit ?? ""}`}</span>
    </label>
  );
}

/** Spectre avant (gris) / après (couleur d'accent), 24 bandes de 30 Hz à 16 kHz. */
function Spectrum({ before, after }: { before: number[] | null; after: number[] | null }) {
  const w = 480;
  const h = 140;
  const all = [...(before ?? []), ...(after ?? [])];
  if (all.length === 0) return null;
  const max = Math.max(...all);
  const min = Math.max(Math.min(...all), max - 60);
  const y = (v: number) => h - ((Math.max(v, min) - min) / (max - min || 1)) * (h - 10) - 4;
  const path = (vals: number[]) => vals.map((v, i) => `${i ? "L" : "M"}${(i / (vals.length - 1)) * w},${y(v)}`).join(" ");
  const labels = [
    [0, "30"],
    [7, "100"],
    [12, "300"],
    [16, "1k"],
    [20, "4k"],
    [23, "16k"],
  ] as const;
  return (
    <svg className="rm-spectrum" viewBox={`0 0 ${w} ${h + 16}`} role="img" aria-label="spectre">
      {before && <path d={path(before)} className="rm-spec-before" />}
      {after && <path d={path(after)} className="rm-spec-after" />}
      {labels.map(([i, l]) => (
        <text key={l} x={(i / 23) * w} y={h + 13} className="rm-spec-label" textAnchor={i === 0 ? "start" : i === 23 ? "end" : "middle"}>
          {l}
        </text>
      ))}
    </svg>
  );
}

/** Écran Remaster : réglages, écoute avant / après au même volume, export WAV. */
export function RemasterView({ selection, playingId, onToast, onOpenTrack }: Props) {
  const { t, i18n } = useTranslation();
  const id = selection[0] ?? null;
  const [row, setRow] = useState<TrackRow | null>(null);
  const [s, setS] = useState<RemasterSettings>(DEFAULT_SETTINGS);
  const [preset, setPreset] = useState<string>("auto");
  const [prep, setPrep] = useState<Prepared | null>(null);
  const [listening, setListening] = useState(false);
  const [bypass, setBypass] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [spec, setSpec] = useState<{ before: number[] | null; after: number[] | null }>({ before: null, after: null });
  const levelTimer = useRef<number | undefined>(undefined);

  const errText = (e: unknown) => (isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal"));
  const nf = (v: number, d = 1) => v.toLocaleString(i18n.language, { maximumFractionDigits: d, minimumFractionDigits: d });

  // Morceau choisi : ses informations et ses derniers réglages.
  useEffect(() => {
    setPrep(null);
    setSpec({ before: null, after: null });
    setListening(false);
    if (id == null) return setRow(null);
    void libraryRows([id]).then((r) => setRow(r[0] ?? null));
    void remasterGet(id).then((saved) => setS(saved ?? DEFAULT_SETTINGS));
  }, [id]);

  // L'écoute s'arrête si un autre morceau est lancé dans le lecteur.
  useEffect(() => {
    if (listening && playingId !== id) setListening(false);
  }, [playingId, id, listening]);

  /** Changement de réglage : appliqué tout de suite à l'écoute, volume recalculé ensuite. */
  const change = useCallback(
    (next: RemasterSettings) => {
      setS(next);
      if (!listening || id == null) return;
      remasterUpdate(next);
      window.clearTimeout(levelTimer.current);
      levelTimer.current = window.setTimeout(() => {
        void remasterLevel(id, next).then(setPrep).catch(() => {});
      }, 900);
    },
    [listening, id],
  );
  const set = (patch: Partial<RemasterSettings>) => change({ ...s, ...patch });
  const setBand = (i: number, gainDb: number) => change({ ...s, bands: s.bands.map((b, k) => (k === i ? { ...b, gainDb } : b)) });

  const applyPreset = async () => {
    if (id == null) return;
    setBusy("auto");
    try {
      change(await remasterAuto(id, preset));
      onToast(t("remaster.presetApplied", { preset: t(`remaster.presets.${preset}`) }));
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const listen = async () => {
    if (id == null) return;
    if (listening) {
      playerCommand("stop");
      setListening(false);
      return;
    }
    setBusy("listen");
    try {
      setPrep(await remasterListen(id, s));
      setListening(true);
      setBypass(false);
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const toggleAb = (v: boolean) => {
    setBypass(v);
    remasterAb(v);
  };

  const compare = async () => {
    if (id == null) return;
    setBusy("spectrum");
    try {
      const p = prep ?? (await remasterLevel(id, s));
      setPrep(p);
      const [before, after] = await Promise.all([remasterSpectrum(id, null, 0), remasterSpectrum(id, s, p.gainDb)]);
      // Même échelle : le spectre « avant » est décalé du gain de volume pour comparer les formes.
      setSpec({ before: before.map((v) => v + (p.afterLufs - p.beforeLufs)), after });
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const exportWav = async () => {
    if (id == null) return;
    setBusy("export");
    try {
      const r = await remasterExport(id, s);
      onToast(t("remaster.exported", { name: r.fileName, lufs: nf(r.lufs), peak: nf(r.peakDb) }), { label: t("remaster.open"), run: () => onOpenTrack(r.trackId) });
    } catch (e) {
      onToast(errText(e), undefined, "error");
    } finally {
      setBusy(null);
    }
  };

  const batch = async () => {
    const ok = await remasterBatch(selection, preset);
    onToast(ok ? t("remaster.batchStarted", { count: selection.length }) : t("remaster.batchRunning"));
  };

  if (id == null) {
    return (
      <div className="settings remaster">
        <h1>{t("nav.remaster")}</h1>
        <div className="empty small-empty">
          <WandSparkles size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("remaster.pick")}</p>
        </div>
        <p className="note-info small"><Info size={14} aria-hidden /> {t("remaster.limits")}</p>
      </div>
    );
  }

  const hpOptions = [0, 20, 25, 30, 40];
  const monoOptions = [0, 80, 100, 120, 150, 200];
  const targets = [null, -14, -12, -10, -9, -8, -7, -6];

  return (
    <div className="settings remaster">
      <h1>{t("nav.remaster")}</h1>
      <p className="rm-track">
        <strong>{row?.title ?? row?.fileName ?? "…"}</strong>
        {row?.version && <span className="muted"> ({row.version})</span>}
        <span className="muted"> — {row?.artists.join(", ")}</span>
        {selection.length > 1 && <span className="muted small"> · {t("remaster.firstOfSelection", { count: selection.length })}</span>}
      </p>

      <div className="folder-actions rm-transport">
        <button className="btn btn-primary" disabled={busy != null} onClick={() => void listen()}>
          {listening ? <Square size={16} aria-hidden /> : <Play size={16} aria-hidden />} {busy === "listen" ? t("remaster.preparing") : listening ? t("remaster.stop") : t("remaster.listen")}
        </button>
        <div className="segmented" role="group" aria-label={t("remaster.ab")}>
          <button aria-pressed={bypass} disabled={!listening} onClick={() => toggleAb(true)}>{t("remaster.before")}</button>
          <button aria-pressed={!bypass} disabled={!listening} onClick={() => toggleAb(false)}>{t("remaster.after")}</button>
        </div>
        {prep && (
          <span className="muted small tabular">
            {t("remaster.levels", { before: nf(prep.beforeLufs), after: nf(prep.afterLufs), peak: nf(prep.afterPeakDb) })}
          </span>
        )}
      </div>
      {listening && <p className="muted small">{t("remaster.abHint")}</p>}

      <div className="folder-actions">
        <label className="chip-select">
          {t("remaster.preset")}
          <select value={preset} onChange={(e) => setPreset(e.target.value)}>
            {PRESETS.map((p) => (
              <option key={p} value={p}>{t(`remaster.presets.${p}`)}</option>
            ))}
          </select>
        </label>
        <button className="btn btn-small" disabled={busy != null} onClick={() => void applyPreset()}>
          <WandSparkles size={14} aria-hidden /> {busy === "auto" ? t("remaster.analyzing") : t("remaster.applyPreset")}
        </button>
        <button className="btn btn-small" onClick={() => change(DEFAULT_SETTINGS)}>
          <RotateCcw size={14} aria-hidden /> {t("remaster.reset")}
        </button>
      </div>

      <div className="rm-grid">
        <section className="rm-card">
          <h2>
            <label><input type="checkbox" checked={s.eqOn} onChange={(e) => set({ eqOn: e.target.checked })} /> {t("remaster.eq")}</label>
          </h2>
          {s.bands.map((b, i) => (
            <Slider
              key={i}
              label={`${t(`remaster.bands.${BAND_LABELS[i]}`)} · ${b.freq >= 1000 ? `${b.freq / 1000} kHz` : `${b.freq} Hz`}`}
              value={b.gainDb}
              min={-12}
              max={12}
              step={0.5}
              disabled={!s.eqOn}
              format={(v) => `${v > 0 ? "+" : ""}${nf(v)} dB`}
              onChange={(v) => setBand(i, v)}
            />
          ))}
          <label className="chip-select">
            {t("remaster.highpass")}
            <select value={s.highpassHz} onChange={(e) => set({ highpassHz: Number(e.target.value) })}>
              {hpOptions.map((h) => (
                <option key={h} value={h}>{h ? `${h} Hz` : t("remaster.off")}</option>
              ))}
            </select>
          </label>
        </section>

        <section className="rm-card">
          <h2>{t("remaster.stereo")}</h2>
          <Slider label={t("remaster.width")} value={Math.round(s.width * 100)} min={0} max={150} step={5} unit=" %" onChange={(v) => set({ width: v / 100 })} />
          <label className="chip-select">
            {t("remaster.monoBass")}
            <select value={s.monoBassHz} onChange={(e) => set({ monoBassHz: Number(e.target.value) })}>
              {monoOptions.map((h) => (
                <option key={h} value={h}>{h ? t("remaster.below", { hz: h }) : t("remaster.off")}</option>
              ))}
            </select>
          </label>
          <label className="checks">
            <span><input type="checkbox" checked={s.width === 0} onChange={(e) => set({ width: e.target.checked ? 0 : 1 })} /> {t("remaster.fullMono")}</span>
          </label>
          <p className="muted small">{t("remaster.stereoHint")}</p>
        </section>

        <section className="rm-card">
          <h2>
            <label><input type="checkbox" checked={s.compOn} onChange={(e) => set({ compOn: e.target.checked })} /> {t("remaster.comp")}</label>
          </h2>
          <Slider label={t("remaster.threshold")} value={s.thresholdDb} min={-40} max={0} step={0.5} disabled={!s.compOn} format={(v) => `${nf(v)} dB`} onChange={(v) => set({ thresholdDb: v })} />
          <Slider label={t("remaster.ratio")} value={s.ratio} min={1} max={8} step={0.1} disabled={!s.compOn} format={(v) => `${nf(v)} : 1`} onChange={(v) => set({ ratio: v })} />
          <Slider label={t("remaster.attack")} value={s.attackMs} min={1} max={100} step={1} unit=" ms" disabled={!s.compOn} onChange={(v) => set({ attackMs: v })} />
          <Slider label={t("remaster.release")} value={s.releaseMs} min={50} max={1000} step={10} unit=" ms" disabled={!s.compOn} onChange={(v) => set({ releaseMs: v })} />
          <Slider label={t("remaster.makeup")} value={s.makeupDb} min={0} max={12} step={0.5} disabled={!s.compOn} format={(v) => `+${nf(v)} dB`} onChange={(v) => set({ makeupDb: v })} />
        </section>

        <section className="rm-card">
          <h2>{t("remaster.loudness")}</h2>
          <label className="chip-select">
            {t("remaster.target")}
            <select value={s.targetLufs ?? ""} onChange={(e) => set({ targetLufs: e.target.value === "" ? null : Number(e.target.value) })}>
              {targets.map((v) => (
                <option key={String(v)} value={v ?? ""}>{v == null ? t("remaster.noTarget") : `${v} LUFS`}</option>
              ))}
            </select>
          </label>
          <label className="checks">
            <span><input type="checkbox" checked={s.limiterOn} onChange={(e) => set({ limiterOn: e.target.checked })} /> {t("remaster.limiter")}</span>
          </label>
          <Slider label={t("remaster.ceiling")} value={s.ceilingDb} min={-3} max={-0.1} step={0.1} disabled={!s.limiterOn} format={(v) => `${nf(v)} dB`} onChange={(v) => set({ ceilingDb: v })} />
          <p className="muted small">{t("remaster.loudnessHint")}</p>
        </section>
      </div>

      <section className="rm-card">
        <div className="folder-actions">
          <h2>{t("remaster.spectrum")}</h2>
          <span className="spacer" />
          <button className="btn btn-small" disabled={busy != null} onClick={() => void compare()}>
            <Waves size={14} aria-hidden /> {busy === "spectrum" ? t("remaster.computing") : t("remaster.compare")}
          </button>
        </div>
        {spec.before ? (
          <>
            <Spectrum before={spec.before} after={spec.after} />
            <p className="muted small"><span className="rm-key before" /> {t("remaster.before")} <span className="rm-key after" /> {t("remaster.after")}</p>
          </>
        ) : (
          <p className="muted small">{t("remaster.spectrumHint")}</p>
        )}
      </section>

      <div className="folder-actions rm-export">
        <button className="btn btn-primary" disabled={busy != null} onClick={() => void exportWav()}>
          <FileDown size={16} aria-hidden /> {busy === "export" ? t("remaster.exporting") : t("remaster.export")}
        </button>
        {selection.length > 1 && (
          <button className="btn" disabled={busy != null} onClick={() => void batch()}>
            <WandSparkles size={16} aria-hidden /> {t("remaster.batch", { count: selection.length, preset: t(`remaster.presets.${preset}`) })}
          </button>
        )}
      </div>
      <p className="note-info small"><Info size={14} aria-hidden /> {t("remaster.exportHint")}</p>
      <p className="note-info small"><Info size={14} aria-hidden /> {t("remaster.limits")}</p>
    </div>
  );
}
