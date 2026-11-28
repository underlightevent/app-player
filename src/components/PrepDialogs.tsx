import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { ArrowRightLeft, Download, FolderOpen, Play, Scissors, Sparkles, TriangleAlert } from "lucide-react";
import { Modal } from "./Modal";
import { isCoreError } from "../core/api";
import { formatSize } from "../core/format";
import { libraryRows, pickFolder, type TrackRow } from "../core/library";
import {
  SECTION_COLORS,
  djeditExport,
  setGenerate,
  similarTracks,
  stemsInstall,
  stemsSeparate,
  stemsStatus,
  structureGet,
  transitionPlay,
  usbCheck,
  type Curve,
  type Similar,
  type StemsKind,
  type StemsStatus,
  type Structure,
  type UsbProfile,
  type UsbReport,
} from "../core/prep";

type Toast = (msg: string, action?: { label: string; run: () => void }, kind?: "error") => void;

export const prepError = (t: TFunction, e: unknown) => {
  if (isCoreError(e)) {
    // Erreurs connues de la phase 8 (détail court venant du cœur).
    const d = e.detail ?? "";
    if (["tempo", "boucle", "court", "stems_missing"].includes(d)) return t(`prep.errors.${d}`);
    return t(`errors.${e.code}`, { defaultValue: e.detail });
  }
  return t("errors.internal");
};

const label = (r: TrackRow | undefined) => (r ? `${r.mainArtist ? `${r.mainArtist} – ` : ""}${r.title ?? r.fileName}${r.version ? ` (${r.version})` : ""}` : "…");

/** Bande colorée des sections d'un morceau (détail, DJ edit). */
export function SectionsBar({ st }: { st: Structure }) {
  const { t } = useTranslation();
  const total = st.durationMs || 1;
  return (
    <div className="sections-bar" role="img" aria-label={st.sections.map((s) => `${t(`prep.sections.${s.kind}`)} ${s.bars}`).join(", ")}>
      {st.sections.map((s, i) => (
        <span
          key={i}
          title={`${t(`prep.sections.${s.kind}`)} · ${t("prep.bars", { count: s.bars })}`}
          style={{ left: `${(s.startMs / total) * 100}%`, width: `${((s.endMs - s.startMs) / total) * 100}%`, background: SECTION_COLORS[s.kind] }}
        />
      ))}
    </div>
  );
}

/** Morceaux similaires à `id` : écoute, essai d'enchaînement. */
export function SimilarDialog({ id, onClose, onPlay, onToast }: { id: number; onClose: () => void; onPlay: (id: number) => void; onToast: Toast }) {
  const { t, i18n } = useTranslation();
  const [list, setList] = useState<Similar[] | null>(null);
  const [rows, setRows] = useState<Map<number, TrackRow>>(new Map());
  const [ref, setRef] = useState<TrackRow | undefined>();
  useEffect(() => {
    void similarTracks(id).then(async (l) => {
      setList(l);
      const r = await libraryRows([id, ...l.map((s) => s.id)]);
      setRows(new Map(r.map((x) => [x.id, x])));
      setRef(r.find((x) => x.id === id));
    });
  }, [id]);
  const tryMix = async (b: number) => {
    try {
      await transitionPlay(id, b, 16);
      onToast(t("prep.transitionStarted"));
    } catch (e) {
      onToast(prepError(t, e), undefined, "error");
    }
  };
  return (
    <Modal wide title={t("prep.similarTitle", { name: label(ref) })} onClose={onClose}>
      <p className="hint">{t("prep.similarHint")}</p>
      <div className="similar-list" aria-busy={list == null}>
        {list?.length === 0 && <p className="muted">{t("prep.similarNone")}</p>}
        {list?.map((s) => {
          const r = rows.get(s.id);
          return (
            <div key={s.id} className="similar-row">
              <span className="similar-score tabular" title={t("prep.score")}>{s.score}</span>
              <span className="similar-title">{label(r)}</span>
              <span className="muted small tabular">
                {r?.bpm ? `${r.bpm.toLocaleString(i18n.language, { maximumFractionDigits: 1 })} BPM` : ""}
                {s.bpmDiff != null && Math.abs(s.bpmDiff) >= 0.1 ? ` (${s.bpmDiff > 0 ? "+" : ""}${s.bpmDiff.toLocaleString(i18n.language)} %)` : ""}
              </span>
              <span className={`compat ${{ same: "compat-good", adjacent: "compat-good", relative: "compat-good", energy: "compat-mid", clash: "compat-bad" }[s.keyCompat ?? ""] ?? ""}`} title={t(`sets.compat.${s.keyCompat ?? "unknown"}`)}>{t(`prep.compat.${s.keyCompat ?? "unknown"}`)}</span>
              {s.sound != null && <span className="muted small tabular" title={t("prep.soundHint")}>{t("prep.sound", { value: s.sound })}</span>}
              <span className="similar-actions">
                <button className="icon-btn" onClick={() => onPlay(s.id)} aria-label={t("player.play")} title={t("player.play")}>
                  <Play size={16} aria-hidden />
                </button>
                <button className="icon-btn" onClick={() => void tryMix(s.id)} aria-label={t("prep.tryTransition")} title={t("prep.tryTransition")}>
                  <ArrowRightLeft size={16} aria-hidden />
                </button>
              </span>
            </div>
          );
        })}
      </div>
    </Modal>
  );
}

/** DJ edit : intro / outro allongées en répétant des mesures. */
export function DjEditDialog({ row, onClose, onToast, onOpenTrack }: { row: TrackRow; onClose: () => void; onToast: Toast; onOpenTrack: (id: number) => void }) {
  const { t } = useTranslation();
  const [st, setSt] = useState<Structure | null | undefined>(undefined);
  const [intro, setIntro] = useState(16);
  const [outro, setOutro] = useState(0);
  const [loop, setLoop] = useState(8);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    void structureGet(row.id).then(setSt).catch(() => setSt(null));
  }, [row.id]);
  const options = [0, 8, 16, 32, 64].filter((n) => n % loop === 0);
  const extraMs = st ? (intro + outro) * (240_000 / st.bpm) : 0;
  const run = async () => {
    setBusy(true);
    try {
      const r = await djeditExport(row.id, { introBars: intro, outroBars: outro, loopBars: loop });
      onToast(t("prep.djeditDone", { name: r.fileName }), { label: t("remaster.open"), run: () => onOpenTrack(r.trackId) });
      onClose();
    } catch (e) {
      onToast(prepError(t, e), undefined, "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={t("prep.djeditTitle", { name: label(row) })}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || intro + outro === 0} onClick={() => void run()}>
            <Scissors size={16} aria-hidden /> {busy ? t("prep.working") : t("prep.djeditRun")}
          </button>
        </>
      }
    >
      <p className="hint">{t("prep.djeditHint")}</p>
      {st && <SectionsBar st={st} />}
      {st === null && <p className="hint small">{t("prep.structureWillRun")}</p>}
      {st && st.confidence < 0.15 && <p className="text-warning small"><TriangleAlert size={14} aria-hidden /> {t("prep.gridUncertain")}</p>}
      <div className="form-grid">
        <label className="chip-select">
          {t("prep.loopBars")}
          <select value={loop} onChange={(e) => { const l = Number(e.target.value); setLoop(l); setIntro((v) => v - (v % l)); setOutro((v) => v - (v % l)); }}>
            {[4, 8, 16].map((n) => <option key={n} value={n}>{t("prep.bars", { count: n })}</option>)}
          </select>
        </label>
        <label className="chip-select">
          {t("prep.introExtra")}
          <select value={intro} onChange={(e) => setIntro(Number(e.target.value))}>
            {options.map((n) => <option key={n} value={n}>{n ? `+ ${t("prep.bars", { count: n })}` : t("prep.none")}</option>)}
          </select>
        </label>
        <label className="chip-select">
          {t("prep.outroExtra")}
          <select value={outro} onChange={(e) => setOutro(Number(e.target.value))}>
            {options.map((n) => <option key={n} value={n}>{n ? `+ ${t("prep.bars", { count: n })}` : t("prep.none")}</option>)}
          </select>
        </label>
      </div>
      {st && intro + outro > 0 && <p className="small tabular">{t("prep.djeditLonger", { seconds: Math.round(extraMs / 1000) })}</p>}
      <p className="hint small">{t("prep.newFileHint")}</p>
    </Modal>
  );
}

/** Séparation voix / instrumental (téléchargement du modèle à la première utilisation). */
export function StemsDialog({ row, onClose, onToast }: { row: TrackRow | null; onClose: () => void; onToast: Toast }) {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState<StemsStatus | null>(null);
  const [kind, setKind] = useState<StemsKind>("both");
  const refresh = () => void stemsStatus().then(setStatus);
  useEffect(refresh, []);
  const install = async () => {
    onToast((await stemsInstall()) ? t("prep.stemsInstalling") : t("prep.stemsBusy"));
    onClose();
  };
  const run = async () => {
    if (!row) return;
    try {
      onToast((await stemsSeparate(row.id, kind)) ? t("prep.stemsStarted") : t("prep.stemsBusy"));
      onClose();
    } catch (e) {
      onToast(prepError(t, e), undefined, "error");
    }
  };
  return (
    <Modal
      title={row ? t("prep.stemsTitle", { name: label(row) }) : t("prep.stemsSetup")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          {status?.supported && !status.installed && (
            <button className="btn btn-primary" onClick={() => void install()}>
              <Download size={16} aria-hidden /> {t("prep.stemsDownload", { size: formatSize(status.downloadBytes, i18n.language) })}
            </button>
          )}
          {status?.installed && row && (
            <button className="btn btn-primary" onClick={() => void run()}>
              <Sparkles size={16} aria-hidden /> {t("prep.stemsRun")}
            </button>
          )}
        </>
      }
    >
      {status && !status.supported && <p className="text-warning">{t("prep.stemsUnsupported")}</p>}
      {status?.supported && !status.installed && (
        <>
          <p>{t("prep.stemsAbout")}</p>
          <p className="hint">{t("prep.stemsConsent", { size: formatSize(status.downloadBytes, i18n.language) })}</p>
        </>
      )}
      {status?.installed && row && (
        <>
          <fieldset className="radio-list">
            {(["both", "instrumental", "acapella", "all"] as StemsKind[]).map((k) => (
              <label key={k}>
                <input type="radio" name="stems" checked={kind === k} onChange={() => setKind(k)} /> {t(`prep.stemsKinds.${k}`)}
              </label>
            ))}
          </fieldset>
          <p className="hint small">{t("prep.stemsSlow")}</p>
          <p className="hint small">{t("prep.newFileHint")}</p>
        </>
      )}
      {status?.installed && !row && <p>{t("prep.stemsReady")}</p>}
    </Modal>
  );
}

const PROFILES: UsbProfile[] = ["old", "nxs2", "cdj3000"];

/** Contrôle d'une clé USB (ou d'un dossier) avant une soirée. Lecture seule. */
export function UsbCheckDialog({ onClose }: { onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const [path, setPath] = useState("");
  const [profile, setProfile] = useState<UsbProfile>("nxs2");
  const [report, setReport] = useState<UsbReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = async (p = path) => {
    if (!p) return;
    setBusy(true);
    setError(null);
    try {
      setReport(await usbCheck(p, profile));
    } catch (e) {
      setError(prepError(t, e));
    } finally {
      setBusy(false);
    }
  };
  const kinds = report ? [...new Set(report.issues.map((i) => i.kind))] : [];
  return (
    <Modal wide title={t("prep.usbTitle")} onClose={onClose}>
      <p className="hint">{t("prep.usbHint")}</p>
      <div className="folder-actions">
        <button
          className="btn btn-small"
          onClick={async () => {
            const p = await pickFolder(t("prep.usbPick"));
            if (p) {
              setPath(p);
              void run(p);
            }
          }}
        >
          <FolderOpen size={14} aria-hidden /> {t("prep.usbPick")}
        </button>
        <span className="muted small">{path || t("prep.usbNone")}</span>
        <span className="spacer" />
        <label className="chip-select">
          {t("prep.usbProfile")}
          <select value={profile} onChange={(e) => setProfile(e.target.value as UsbProfile)}>
            {PROFILES.map((p) => <option key={p} value={p}>{t(`prep.profiles.${p}`)}</option>)}
          </select>
        </label>
        <button className="btn btn-small btn-primary" disabled={!path || busy} onClick={() => void run()}>{busy ? t("prep.working") : t("prep.usbRun")}</button>
      </div>
      {error && <p className="text-error">{error}</p>}
      {report && (
        <div className="usb-report">
          <p className={report.issues.length === 0 && report.filesystemOk !== false ? "text-success" : ""}>
            <strong>{t("prep.usbSummary", { ok: report.ok, files: report.files, size: formatSize(report.bytes, i18n.language) })}</strong>
          </p>
          {report.filesystem && (
            <p className={report.filesystemOk === false ? "text-error" : "small"}>
              {report.filesystemOk === false ? t("prep.usbFsBad", { fs: report.filesystem }) : t("prep.usbFsOk", { fs: report.filesystem })}
            </p>
          )}
          <p className="small muted">{report.rekordboxExport ? t("prep.usbRekordbox") : t("prep.usbNoRekordbox")}</p>
          {kinds.map((k) => (
            <details key={k} open={kinds.length < 3}>
              <summary>
                <TriangleAlert size={14} aria-hidden /> {t(`prep.usbKinds.${k}`)} — {report.issues.filter((i) => i.kind === k).length}
              </summary>
              <p className="hint small">{t(`prep.usbFix.${k}`)}</p>
              <ul className="usb-issues small">
                {report.issues.filter((i) => i.kind === k).slice(0, 200).map((i) => (
                  <li key={i.path}><span className="path">{i.path}</span> <span className="muted">{i.detail}</span></li>
                ))}
              </ul>
            </details>
          ))}
        </div>
      )}
    </Modal>
  );
}

const CURVES: Curve[] = ["rise", "plateau", "wave", "fall"];

/** Générateur de set : durée, courbe d'énergie, tempo ; parmi les morceaux affichés. */
export function SetGeneratorDialog({ pool, start, onClose, onDone, onToast }: { pool: number[]; start: number | null; onClose: () => void; onDone: (setId: number) => void; onToast: Toast }) {
  const { t, i18n } = useTranslation();
  const [name, setName] = useState(t("prep.genDefaultName", { date: new Date().toLocaleDateString() }));
  const [minutes, setMinutes] = useState(60);
  const [curve, setCurve] = useState<Curve>("rise");
  const [bpmMin, setBpmMin] = useState("");
  const [bpmMax, setBpmMax] = useState("");
  const [useStart, setUseStart] = useState(start != null);
  const [busy, setBusy] = useState(false);
  const num = (v: string) => (v.trim() ? Number(v.replace(",", ".")) : null);
  const run = async () => {
    setBusy(true);
    try {
      const [id, n] = await setGenerate(name.trim() || "Set", { minutes, curve, bpmMin: num(bpmMin), bpmMax: num(bpmMax), pool, start: useStart ? start : null });
      if (n === 0) {
        onToast(t("prep.genEmpty"), undefined, "error");
        return;
      }
      onToast(t("prep.genDone", { count: n }));
      onDone(id);
      onClose();
    } catch (e) {
      onToast(prepError(t, e), undefined, "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal
      title={t("prep.genTitle")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || pool.length < 2} onClick={() => void run()}>{busy ? t("prep.working") : t("prep.genRun")}</button>
        </>
      }
    >
      <p className="hint">{t("prep.genHint", { count: pool.length })}</p>
      <div className="form-grid">
        <label className="form-row">
          <span>{t("prep.genName")}</span>
          <input className="text-input" value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="chip-select">
          {t("prep.genMinutes")}
          <select value={minutes} onChange={(e) => setMinutes(Number(e.target.value))}>
            {[30, 45, 60, 90, 120, 180, 240].map((m) => <option key={m} value={m}>{m >= 60 ? `${(m / 60).toLocaleString(i18n.language)} h` : `${m} min`}</option>)}
          </select>
        </label>
        <label className="chip-select">
          {t("prep.genCurve")}
          <select value={curve} onChange={(e) => setCurve(e.target.value as Curve)}>
            {CURVES.map((c) => <option key={c} value={c}>{t(`prep.curves.${c}`)}</option>)}
          </select>
        </label>
        <label className="chip-select">
          {t("prep.genBpm")}
          <input className="text-input bpm-input" inputMode="decimal" placeholder={t("prep.min")} value={bpmMin} onChange={(e) => setBpmMin(e.target.value)} />
          –
          <input className="text-input bpm-input" inputMode="decimal" placeholder={t("prep.max")} value={bpmMax} onChange={(e) => setBpmMax(e.target.value)} />
        </label>
        {start != null && (
          <label className="checks">
            <span><input type="checkbox" checked={useStart} onChange={(e) => setUseStart(e.target.checked)} /> {t("prep.genStart")}</span>
          </label>
        )}
      </div>
      <p className="hint small">{t("prep.genAfter")}</p>
    </Modal>
  );
}
