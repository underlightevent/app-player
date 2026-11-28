import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileDown, Info, Play, RotateCcw, Save, Scissors, Square, Trash2, WandSparkles } from "lucide-react";
import { getSettings, isCoreError, setSetting } from "../core/api";
import { SpectrumPanel } from "./SpectrumPanel";
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
  remasterUpdate,
  parseUserPresets,
  type Prepared,
  type RemasterSettings,
  type UserPreset,
} from "../core/remaster";

interface Props {
  selection: number[];
  /** Morceau en cours dans le lecteur du bas (pour savoir si l'écoute Remaster joue). */
  playingId: number | null;
  onToast: (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;
  onOpenTrack: (id: number) => void;
}

const BAND_LABELS = ["low", "lowMid", "mid", "highMid", "high"];

const decimals = (step: number) => (String(step).split(".")[1] ?? "").length;

/** Champ chiffré : la saisie est validée à Entrée ou en quittant le champ (ramenée dans les bornes). */
export function NumInput(p: { value: number; min: number; max: number; step: number; disabled?: boolean; label: string; onChange: (v: number) => void }) {
  const { i18n } = useTranslation();
  const d = decimals(p.step);
  // Virgule décimale en français.
  const shown = p.value.toFixed(d).replace(".", i18n.language.startsWith("fr") ? "," : ".");
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft == null) return;
    const v = Number(draft.replace(",", "."));
    setDraft(null);
    if (!Number.isFinite(v)) return;
    const k = Math.round(v / p.step) * p.step;
    p.onChange(Number(Math.min(p.max, Math.max(p.min, k)).toFixed(d)));
  };
  return (
    <input
      className="rm-num tabular"
      type="text"
      inputMode="decimal"
      aria-label={p.label}
      disabled={p.disabled}
      value={draft ?? shown}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(null);
        if (e.key === "ArrowUp" || e.key === "ArrowDown") {
          e.preventDefault();
          const base = draft != null ? Number(draft.replace(",", ".")) || p.value : p.value;
          const k = base + (e.key === "ArrowUp" ? p.step : -p.step) * (e.shiftKey ? 10 : 1);
          setDraft(null);
          p.onChange(Number(Math.min(p.max, Math.max(p.min, k)).toFixed(d)));
        }
      }}
    />
  );
}

/** Curseur avec libellé, champ chiffré modifiable et unité. */
function Slider(p: { label: string; value: number; min: number; max: number; step: number; unit?: string; disabled?: boolean; onChange: (v: number) => void }) {
  return (
    <div className={`rm-slider${p.disabled ? " disabled" : ""}`}>
      <span className="rm-slider-label">{p.label}</span>
      <input type="range" aria-label={p.label} min={p.min} max={p.max} step={p.step} value={p.value} disabled={p.disabled} onChange={(e) => p.onChange(Number(e.target.value))} />
      <NumInput label={p.label} value={p.value} min={p.min} max={p.max} step={p.step} disabled={p.disabled} onChange={p.onChange} />
      <span className="rm-slider-unit muted">{p.unit ?? ""}</span>
    </div>
  );
}

/** Curseur de fréquence : échelle logarithmique (20 Hz à 20 kHz), saisie exacte en Hz. */
function FreqSlider(p: { label: string; value: number; disabled?: boolean; onChange: (v: number) => void }) {
  const lo = Math.log(20);
  const hi = Math.log(20000);
  const pos = Math.round(((Math.log(Math.max(20, p.value)) - lo) / (hi - lo)) * 1000);
  return (
    <div className={`rm-slider rm-sub${p.disabled ? " disabled" : ""}`}>
      <span className="rm-slider-label">{p.label}</span>
      <input type="range" aria-label={p.label} min={0} max={1000} step={1} value={pos} disabled={p.disabled} onChange={(e) => p.onChange(Math.round(Math.exp(lo + (Number(e.target.value) / 1000) * (hi - lo))))} />
      <NumInput label={p.label} value={p.value} min={20} max={20000} step={1} disabled={p.disabled} onChange={p.onChange} />
      <span className="rm-slider-unit muted">Hz</span>
    </div>
  );
}

/** 83,4 s → « 1:23,4 ». */
function clock(sec: number, lang: string) {
  const m = Math.floor(sec / 60);
  const r = sec - m * 60;
  return `${m}:${r.toLocaleString(lang, { minimumIntegerDigits: 2, minimumFractionDigits: 1, maximumFractionDigits: 1 })}`;
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
  const levelTimer = useRef<number | undefined>(undefined);
  const [userPresets, setUserPresets] = useState<UserPreset[]>([]);
  const [naming, setNaming] = useState<string | null>(null);

  useEffect(() => {
    void getSettings().then((v) => setUserPresets(parseUserPresets(v.remaster_presets)));
  }, []);
  const saveUserPresets = async (list: UserPreset[]) => {
    setUserPresets(list);
    await setSetting("remaster_presets", JSON.stringify(list));
  };

  const errText = (e: unknown) => (isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal"));
  const nf = (v: number, d = 1) => v.toLocaleString(i18n.language, { maximumFractionDigits: d, minimumFractionDigits: d });

  // Morceau choisi : ses informations et ses derniers réglages.
  useEffect(() => {
    setPrep(null);
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
  const setBand = (i: number, patch: { gainDb?: number; freq?: number }) => change({ ...s, bands: s.bands.map((b, k) => (k === i ? { ...b, ...patch } : b)) });
  const userPreset = preset.startsWith("user:") ? userPresets.find((p) => p.name === preset.slice(5)) ?? null : null;
  const presetLabel = userPreset ? userPreset.name : t(`remaster.presets.${preset}`, { defaultValue: preset });
  /** La coupe (début / fin) est propre au morceau : un préréglage ne la change pas. */
  const keepTrim = (next: RemasterSettings) => ({ ...next, startS: s.startS, endS: s.endS });

  const applyPreset = async () => {
    if (id == null) return;
    if (userPreset) {
      change(keepTrim(userPreset.settings));
      onToast(t("remaster.presetApplied", { preset: userPreset.name }));
      return;
    }
    setBusy("auto");
    try {
      // Les préréglages intégrés ne touchent ni à la coupe ni aux fondus.
      change({ ...(await remasterAuto(id, preset)), startS: s.startS, endS: s.endS, fadeInS: s.fadeInS, fadeOutS: s.fadeOutS });
      onToast(t("remaster.presetApplied", { preset: presetLabel }));
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

  const savePreset = async () => {
    const name = (naming ?? "").trim();
    if (!name) return setNaming(null);
    const settings = { ...s, startS: 0, endS: 0 };
    await saveUserPresets([...userPresets.filter((p) => p.name !== name), { name, settings }].sort((a, b) => a.name.localeCompare(b.name)));
    setPreset(`user:${name}`);
    setNaming(null);
    onToast(t("remaster.presetSaved", { name }));
  };

  const deletePreset = async () => {
    if (!userPreset) return;
    const before = userPresets;
    await saveUserPresets(userPresets.filter((p) => p.name !== userPreset.name));
    setPreset("auto");
    onToast(t("remaster.presetDeleted", { name: userPreset.name }), { label: t("remaster.undo"), run: () => void saveUserPresets(before) });
  };

  const batch = async () => {
    const ok = await remasterBatch(selection, preset, userPreset ? userPreset.settings : null);
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

  const dur = row?.durationMs ? Math.floor(row.durationMs / 100) / 10 : 0;
  const maxT = Math.max(dur, s.startS, s.endS, 1);
  const keptS = Math.max(0, (s.endS > 0 ? s.endS : dur) - s.startS);

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
            <optgroup label={t("remaster.builtinPresets")}>
              {PRESETS.map((p) => (
                <option key={p} value={p}>{t(`remaster.presets.${p}`)}</option>
              ))}
            </optgroup>
            {userPresets.length > 0 && (
              <optgroup label={t("remaster.myPresets")}>
                {userPresets.map((p) => (
                  <option key={p.name} value={`user:${p.name}`}>{p.name}</option>
                ))}
              </optgroup>
            )}
          </select>
        </label>
        <button className="btn btn-small" disabled={busy != null} onClick={() => void applyPreset()}>
          <WandSparkles size={14} aria-hidden /> {busy === "auto" ? t("remaster.analyzing") : userPreset ? t("remaster.applyUserPreset") : t("remaster.applyPreset")}
        </button>
        {userPreset && (
          <button className="btn btn-small" onClick={() => void deletePreset()} title={t("remaster.deletePreset")}>
            <Trash2 size={14} aria-hidden /> {t("remaster.deletePreset")}
          </button>
        )}
        <button className="btn btn-small" onClick={() => change({ ...DEFAULT_SETTINGS, startS: s.startS, endS: s.endS })}>
          <RotateCcw size={14} aria-hidden /> {t("remaster.reset")}
        </button>
        {naming == null ? (
          <button className="btn btn-small" onClick={() => setNaming(userPreset?.name ?? "")}>
            <Save size={14} aria-hidden /> {t("remaster.savePreset")}
          </button>
        ) : (
          <span className="rm-naming">
            <input
              autoFocus
              type="text"
              maxLength={60}
              placeholder={t("remaster.presetName")}
              value={naming}
              onChange={(e) => setNaming(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void savePreset();
                if (e.key === "Escape") setNaming(null);
              }}
            />
            <button className="btn btn-small btn-primary" disabled={!naming.trim()} onClick={() => void savePreset()}>{t("remaster.save")}</button>
            <button className="btn btn-small" onClick={() => setNaming(null)}>{t("remaster.cancel")}</button>
          </span>
        )}
      </div>
      {naming != null && <p className="muted small">{t("remaster.presetHint")}</p>}

      <SpectrumPanel id={id} settings={s} />

      <div className="rm-grid">
        <section className="rm-card">
          <h2>
            <label><input type="checkbox" checked={s.eqOn} onChange={(e) => set({ eqOn: e.target.checked })} /> {t("remaster.eq")}</label>
          </h2>
          {s.bands.map((b, i) => (
            <div key={i} className="rm-band">
              <Slider label={t(`remaster.bands.${BAND_LABELS[i]}`)} value={b.gainDb} min={-12} max={12} step={0.1} unit="dB" disabled={!s.eqOn} onChange={(v) => setBand(i, { gainDb: v })} />
              <FreqSlider label={t("remaster.freq")} value={b.freq} disabled={!s.eqOn} onChange={(v) => setBand(i, { freq: v })} />
            </div>
          ))}
          <Slider label={t("remaster.highpass")} value={s.highpassHz} min={0} max={200} step={1} unit="Hz" onChange={(v) => set({ highpassHz: v })} />
          <p className="muted small">{t("remaster.zeroOff")}</p>
        </section>

        <section className="rm-card">
          <h2>{t("remaster.stereo")}</h2>
          <Slider label={t("remaster.width")} value={Math.round(s.width * 100)} min={0} max={150} step={1} unit="%" onChange={(v) => set({ width: v / 100 })} />
          <Slider label={t("remaster.monoBass")} value={s.monoBassHz} min={0} max={300} step={1} unit="Hz" onChange={(v) => set({ monoBassHz: v })} />
          <label className="checks">
            <span><input type="checkbox" checked={s.width === 0} onChange={(e) => set({ width: e.target.checked ? 0 : 1 })} /> {t("remaster.fullMono")}</span>
          </label>
          <p className="muted small">{t("remaster.stereoHint")}</p>
        </section>

        <section className="rm-card">
          <h2>
            <label><input type="checkbox" checked={s.compOn} onChange={(e) => set({ compOn: e.target.checked })} /> {t("remaster.comp")}</label>
          </h2>
          <Slider label={t("remaster.threshold")} value={s.thresholdDb} min={-60} max={0} step={0.1} unit="dB" disabled={!s.compOn} onChange={(v) => set({ thresholdDb: v })} />
          <Slider label={t("remaster.ratio")} value={s.ratio} min={1} max={20} step={0.05} unit=": 1" disabled={!s.compOn} onChange={(v) => set({ ratio: v })} />
          <Slider label={t("remaster.attack")} value={s.attackMs} min={0.1} max={200} step={0.1} unit="ms" disabled={!s.compOn} onChange={(v) => set({ attackMs: v })} />
          <Slider label={t("remaster.release")} value={s.releaseMs} min={10} max={2000} step={1} unit="ms" disabled={!s.compOn} onChange={(v) => set({ releaseMs: v })} />
          <Slider label={t("remaster.makeup")} value={s.makeupDb} min={-12} max={24} step={0.1} unit="dB" disabled={!s.compOn} onChange={(v) => set({ makeupDb: v })} />
        </section>

        <section className="rm-card">
          <h2>{t("remaster.loudness")}</h2>
          <label className="checks">
            <span><input type="checkbox" checked={s.targetLufs != null} onChange={(e) => set({ targetLufs: e.target.checked ? -9 : null })} /> {t("remaster.targetOn")}</span>
          </label>
          <Slider label={t("remaster.target")} value={s.targetLufs ?? -9} min={-30} max={-4} step={0.1} unit="LUFS" disabled={s.targetLufs == null} onChange={(v) => set({ targetLufs: v })} />
          <label className="checks">
            <span><input type="checkbox" checked={s.limiterOn} onChange={(e) => set({ limiterOn: e.target.checked })} /> {t("remaster.limiter")}</span>
          </label>
          <Slider label={t("remaster.ceiling")} value={s.ceilingDb} min={-6} max={0} step={0.05} unit="dB" disabled={!s.limiterOn} onChange={(v) => set({ ceilingDb: v })} />
          <p className="muted small">{t("remaster.loudnessHint")}</p>
        </section>

        <section className="rm-card">
          <h2><Scissors size={16} aria-hidden /> {t("remaster.trim")}</h2>
          <Slider label={t("remaster.start")} value={s.startS} min={0} max={maxT} step={0.1} unit="s" onChange={(v) => set({ startS: v, endS: s.endS > 0 && s.endS <= v ? 0 : s.endS })} />
          <Slider label={t("remaster.end")} value={s.endS > 0 ? s.endS : dur} min={0} max={maxT} step={0.1} unit="s" onChange={(v) => set({ endS: v >= dur || v <= s.startS ? 0 : v })} />
          <Slider label={t("remaster.fadeIn")} value={s.fadeInS} min={0} max={30} step={0.1} unit="s" onChange={(v) => set({ fadeInS: v })} />
          <Slider label={t("remaster.fadeOut")} value={s.fadeOutS} min={0} max={30} step={0.1} unit="s" onChange={(v) => set({ fadeOutS: v })} />
          <p className="muted small tabular">
            {t("remaster.kept", { from: clock(s.startS, i18n.language), to: s.endS > 0 ? clock(s.endS, i18n.language) : t("remaster.toEnd"), len: clock(keptS, i18n.language) })}
          </p>
          <p className="muted small">{t("remaster.trimHint")}</p>
        </section>
      </div>

      <div className="folder-actions rm-export">
        <button className="btn btn-primary" disabled={busy != null} onClick={() => void exportWav()}>
          <FileDown size={16} aria-hidden /> {busy === "export" ? t("remaster.exporting") : t("remaster.export")}
        </button>
        {selection.length > 1 && (
          <button className="btn" disabled={busy != null} onClick={() => void batch()}>
            <WandSparkles size={16} aria-hidden /> {t("remaster.batch", { count: selection.length, preset: presetLabel })}
          </button>
        )}
      </div>
      <p className="note-info small"><Info size={14} aria-hidden /> {t("remaster.exportHint")}</p>
      <p className="note-info small"><Info size={14} aria-hidden /> {t("remaster.limits")}</p>
    </div>
  );
}
