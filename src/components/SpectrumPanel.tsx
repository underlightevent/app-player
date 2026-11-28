import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { Pin, PinOff, RefreshCw } from "lucide-react";
import { isCoreError } from "../core/api";
import {
  remasterSpectrum,
  type Compare,
  type RemasterSettings,
} from "../core/remaster";

type Mode = "curves" | "diff" | "stereo";

const PAD_L = 34;
const PAD_B = 18;
const F_MIN = 30;
const F_MAX = 18000;
const TICKS = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
/** Zones pour le tableau de comparaison (Hz). */
const ZONES: [string, number, number][] = [
  ["sub", 30, 60],
  ["low", 60, 250],
  ["lowMid", 250, 1000],
  ["mid", 1000, 4000],
  ["high", 4000, 18000],
];

const pref = (k: string, d: string) => {
  try {
    return localStorage.getItem(`remaster.spectrum.${k}`) ?? d;
  } catch {
    return d;
  }
};
const savePref = (k: string, v: string) => {
  try {
    localStorage.setItem(`remaster.spectrum.${k}`, v);
  } catch {
    /* simple confort */
  }
};

const fLabel = (f: number) => (f >= 1000 ? `${f / 1000}k` : `${Math.round(f)}`);
/** Moyenne en puissance de niveaux en dB. */
const powAvg = (v: number[]) =>
  v.length
    ? 10 * Math.log10(v.reduce((a, d) => a + 10 ** (d / 10), 0) / v.length)
    : -140;

/** Spectre avant / après, recalculé en direct pendant les réglages, avec tableau de comparaison. */
export function SpectrumPanel({
  id,
  settings,
}: {
  id: number;
  settings: RemasterSettings;
}) {
  const { t, i18n } = useTranslation();
  const [data, setData] = useState<Compare | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [live, setLive] = useState(pref("live", "true") === "true");
  const [pinned, setPinned] = useState(pref("pinned", "true") === "true");
  const [matched, setMatched] = useState(pref("matched", "true") === "true");
  const [mode, setMode] = useState<Mode>(
    (pref("mode", "curves") as Mode) || "curves",
  );
  const [hover, setHover] = useState<number | null>(null);
  const [manual, setManual] = useState(0);
  const seq = useRef(0);
  // Taille réelle du graphique (en pixels) : il occupe toute la largeur disponible.
  const box = useRef<HTMLDivElement>(null);
  const [W, setW] = useState(900);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      setW(Math.max(320, Math.round(el.clientWidth))),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, [data != null]);
  const H = pinned ? 230 : 300;
  const x = (f: number) =>
    PAD_L + (Math.log(f / F_MIN) / Math.log(F_MAX / F_MIN)) * (W - PAD_L - 6);
  const key = JSON.stringify(settings);
  const nf = (v: number, d = 1) =>
    v.toLocaleString(i18n.language, {
      maximumFractionDigits: d,
      minimumFractionDigits: d,
    });
  const sign = (v: number, d = 1) => `${v > 0.05 ? "+" : ""}${nf(v, d)}`;

  useEffect(() => setData(null), [id]);

  // Recalcul : 0,4 s après le dernier changement de réglage (en direct), ou à la demande.
  useEffect(() => {
    if (!live && manual === 0) return;
    const n = ++seq.current;
    const timer = window.setTimeout(
      () => {
        setBusy(true);
        remasterSpectrum(id, JSON.parse(key) as RemasterSettings)
          .then((c) => {
            if (n !== seq.current) return;
            setData(c);
            setError(null);
          })
          .catch(
            (e) =>
              n === seq.current &&
              setError(
                isCoreError(e)
                  ? t(`errors.${e.code}`, { defaultValue: e.detail })
                  : t("errors.internal"),
              ),
          )
          .finally(() => n === seq.current && setBusy(false));
      },
      live ? 400 : 0,
    );
    return () => window.clearTimeout(timer);
  }, [id, key, live, manual, t]);

  /** Décalage de l'original pour comparer au même volume perçu. */
  const shift = data && matched ? data.after.lufs - data.before.lufs : 0;

  const view = useMemo(() => {
    if (!data) return null;
    const n = data.freqs.length;
    const before = data.before.bands.map((v) => v + shift);
    const after = data.after.bands;
    let series: { before: number[]; after: number[] };
    let lo: number;
    let hi: number;
    let step: number;
    if (mode === "stereo") {
      series = {
        before: data.before.side.map((v, i) => v - data.before.bands[i]),
        after: data.after.side.map((v, i) => v - data.after.bands[i]),
      };
      lo = -42;
      hi = 6;
      step = 6;
    } else if (mode === "diff") {
      series = {
        before: before.map(() => 0),
        after: after.map((v, i) => v - before[i]),
      };
      const m = Math.max(3, ...series.after.map((v) => Math.abs(v)));
      hi = Math.ceil(m / 3) * 3;
      lo = -hi;
      step = hi > 9 ? 3 : hi > 6 ? 2 : 1;
    } else {
      series = { before, after };
      hi = Math.ceil(Math.max(...before, ...after) / 6) * 6;
      lo = hi - 66;
      step = 6;
    }
    const y = (v: number) =>
      ((hi - Math.min(hi, Math.max(lo, v))) / (hi - lo)) * (H - PAD_B - 6) + 6;
    const line = (v: number[]) =>
      v
        .map(
          (d, i) =>
            `${i ? "L" : "M"}${x(data.freqs[i]).toFixed(1)},${y(d).toFixed(1)}`,
        )
        .join(" ");
    const area = (v: number[]) =>
      `${line(v)} L${x(data.freqs[n - 1]).toFixed(1)},${H - PAD_B} L${x(data.freqs[0]).toFixed(1)},${H - PAD_B} Z`;
    const grid: number[] = [];
    for (let v = hi; v >= lo; v -= step) grid.push(v);
    return { series, y, line, area, grid, lo, hi };
  }, [data, mode, shift, W, H]);

  const onMove = (e: MouseEvent<SVGSVGElement>) => {
    if (!data) return;
    const r = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - r.left) / r.width) * W;
    let best = 0;
    data.freqs.forEach((f, i) => {
      if (Math.abs(x(f) - px) < Math.abs(x(data.freqs[best]) - px)) best = i;
    });
    setHover(px >= PAD_L ? best : null);
  };

  const zone = (lo: number, hi: number) => {
    if (!data) return null;
    const idx = data.freqs
      .map((f, i) => (f >= lo && f < hi ? i : -1))
      .filter((i) => i >= 0);
    const b = powAvg(idx.map((i) => data.before.bands[i] + shift));
    const a = powAvg(idx.map((i) => data.after.bands[i]));
    return a - b;
  };

  const clock = (s: number) =>
    `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  const setP = (k: string, v: string) => savePref(k, v);

  return (
    <>
      <section className={`rm-card rm-spectrum-card${pinned ? " pinned" : ""}`}>
        <div className="rm-spec-head">
          <h2>{t("remaster.spectrum")}</h2>
          <div
            className="segmented"
            role="group"
            aria-label={t("remaster.specView")}
          >
            {(["curves", "diff", "stereo"] as Mode[]).map((m) => (
              <button
                key={m}
                aria-pressed={mode === m}
                onClick={() => (setMode(m), setP("mode", m))}
              >
                {t(`remaster.specMode.${m}`)}
              </button>
            ))}
          </div>
          <label className="checks">
            <span>
              <input
                type="checkbox"
                checked={matched}
                disabled={mode === "stereo"}
                onChange={(e) => (
                  setMatched(e.target.checked),
                  setP("matched", String(e.target.checked))
                )}
              />{" "}
              {t("remaster.sameLoudness")}
            </span>
          </label>
          <label className="checks">
            <span>
              <input
                type="checkbox"
                checked={live}
                onChange={(e) => (
                  setLive(e.target.checked),
                  setP("live", String(e.target.checked))
                )}
              />{" "}
              {t("remaster.live")}
            </span>
          </label>
          <span className="spacer" />
          {busy && (
            <span className="muted small">{t("remaster.computing")}</span>
          )}
          {!live && (
            <button
              className="btn btn-small"
              disabled={busy}
              onClick={() => setManual((n) => n + 1)}
            >
              <RefreshCw size={14} aria-hidden /> {t("remaster.refresh")}
            </button>
          )}
          <button
            className="icon-btn"
            aria-pressed={pinned}
            onClick={() => (
              setPinned(!pinned),
              setP("pinned", String(!pinned))
            )}
            title={t(pinned ? "remaster.unpin" : "remaster.pin")}
            aria-label={t(pinned ? "remaster.unpin" : "remaster.pin")}
          >
            {pinned ? (
              <PinOff size={16} aria-hidden />
            ) : (
              <Pin size={16} aria-hidden />
            )}
          </button>
        </div>

        {error && <p className="text-error small">{error}</p>}
        {!data || !view ? (
          <p className="muted small">
            {live ? t("remaster.computing") : t("remaster.spectrumHint")}
          </p>
        ) : (
          <div className="rm-spec-body">
            <div className="rm-spec-chart" ref={box}>
              <svg
                className="rm-spectrum"
                width={W}
                height={H}
                viewBox={`0 0 ${W} ${H}`}
                role="img"
                aria-label={t("remaster.spectrum")}
                onMouseMove={onMove}
                onMouseLeave={() => setHover(null)}
              >
                {view.grid.map((v) => (
                  <g key={v}>
                    <line
                      x1={PAD_L}
                      x2={W}
                      y1={view.y(v)}
                      y2={view.y(v)}
                      className={
                        v === 0 && mode !== "curves"
                          ? "rm-grid-zero"
                          : "rm-grid-line"
                      }
                    />
                    <text
                      x={PAD_L - 4}
                      y={view.y(v) + 3}
                      className="rm-spec-label"
                      textAnchor="end"
                    >
                      {v > 0 && mode === "diff" ? `+${v}` : v}
                    </text>
                  </g>
                ))}
                {TICKS.map((f) => (
                  <g key={f}>
                    <line
                      x1={x(f)}
                      x2={x(f)}
                      y1={6}
                      y2={H - PAD_B}
                      className="rm-grid-line"
                    />
                    <text
                      x={x(f)}
                      y={H - 4}
                      className="rm-spec-label"
                      textAnchor="middle"
                    >
                      {fLabel(f)}
                    </text>
                  </g>
                ))}
                {mode === "diff" ? (
                  data.freqs.map((f, i) => {
                    const v = view.series.after[i];
                    const x0 = i ? (x(data.freqs[i - 1]) + x(f)) / 2 : x(f) - 6;
                    const x1 =
                      i < data.freqs.length - 1
                        ? (x(f) + x(data.freqs[i + 1])) / 2
                        : x(f) + 6;
                    const y0 = view.y(0);
                    const y1 = view.y(v);
                    return (
                      <rect
                        key={f}
                        x={x0 + 1}
                        width={Math.max(1, x1 - x0 - 2)}
                        y={Math.min(y0, y1)}
                        height={Math.abs(y1 - y0)}
                        className={v >= 0 ? "rm-bar-up" : "rm-bar-down"}
                      />
                    );
                  })
                ) : (
                  <>
                    <path
                      d={view.area(view.series.before)}
                      className="rm-spec-before-area"
                    />
                    <path
                      d={view.line(view.series.before)}
                      className="rm-spec-before"
                    />
                    <path
                      d={view.line(view.series.after)}
                      className="rm-spec-after"
                    />
                  </>
                )}
                {hover != null && (
                  <line
                    x1={x(data.freqs[hover])}
                    x2={x(data.freqs[hover])}
                    y1={6}
                    y2={H - PAD_B}
                    className="rm-hover-line"
                  />
                )}
              </svg>
              {hover != null && (
                <div
                  className="rm-spec-tip small tabular"
                  style={
                    x(data.freqs[hover]) > W - 200
                      ? { right: W - x(data.freqs[hover]) + 8 }
                      : { left: x(data.freqs[hover]) + 8 }
                  }
                >
                  <strong>
                    {data.freqs[hover] >= 1000
                      ? `${nf(data.freqs[hover] / 1000)} kHz`
                      : `${Math.round(data.freqs[hover])} Hz`}
                  </strong>
                  {mode === "stereo" ? (
                    <>
                      <span>
                        {t("remaster.before")} : {nf(view.series.before[hover])}{" "}
                        dB
                      </span>
                      <span>
                        {t("remaster.after")} : {nf(view.series.after[hover])}{" "}
                        dB
                      </span>
                    </>
                  ) : (
                    <>
                      <span>
                        {t("remaster.before")} :{" "}
                        {nf(data.before.bands[hover] + shift)} dB
                      </span>
                      <span>
                        {t("remaster.after")} : {nf(data.after.bands[hover])} dB
                      </span>
                      <span>
                        {t("remaster.diff")} :{" "}
                        {sign(
                          data.after.bands[hover] -
                            data.before.bands[hover] -
                            shift,
                        )}{" "}
                        dB
                      </span>
                    </>
                  )}
                </div>
              )}
            </div>
            <p className="muted small rm-spec-legend">
              {mode === "diff" ? (
                t("remaster.diffHint")
              ) : (
                <>
                  <span className="rm-key before" /> {t("remaster.original")}{" "}
                  <span className="rm-key after" /> {t("remaster.result")}
                  {" · "}
                  {mode === "stereo"
                    ? t("remaster.stereoViewHint")
                    : matched
                      ? t("remaster.matchedHint")
                      : t("remaster.realHint")}
                </>
              )}
            </p>
          </div>
        )}
      </section>
      {data && (
        <section className="rm-card rm-compare-card">
          <table className="rm-compare small tabular">
            <thead>
              <tr>
                <th />
                <th>{t("remaster.original")}</th>
                <th>{t("remaster.result")}</th>
                <th>{t("remaster.diff")}</th>
              </tr>
            </thead>
            <tbody>
              {(
                [
                  ["loudnessRow", data.before.lufs, data.after.lufs, "LUFS"],
                  ["peakRow", data.before.peakDb, data.after.peakDb, "dB"],
                  [
                    "dynamicsRow",
                    data.before.peakDb - data.before.rmsDb,
                    data.after.peakDb - data.after.rmsDb,
                    "dB",
                  ],
                  ["widthRow", data.before.widthPct, data.after.widthPct, "%"],
                ] as const
              ).map(([k, b, a, u]) => (
                <tr key={k}>
                  <th title={t(`remaster.rows.${k}Hint`)}>
                    {t(`remaster.rows.${k}`)}
                  </th>
                  <td>
                    {nf(b, u === "%" ? 0 : 1)} {u}
                  </td>
                  <td>
                    {nf(a, u === "%" ? 0 : 1)} {u}
                  </td>
                  <td className={Math.abs(a - b) >= 0.5 ? "strong" : "muted"}>
                    {sign(a - b, u === "%" ? 0 : 1)} {u}
                  </td>
                </tr>
              ))}
              {ZONES.map(([k, lo, hi]) => {
                const d = zone(lo, hi) ?? 0;
                return (
                  <tr key={k}>
                    <th>
                      {t(`remaster.zones.${k}`)}{" "}
                      <span className="muted">
                        {fLabel(lo)}–{fLabel(hi)} Hz
                      </span>
                    </th>
                    <td colSpan={2}>
                      <span className="rm-zone-bar">
                        <span
                          className={d >= 0 ? "up" : "down"}
                          style={{
                            width: `${Math.min(50, (Math.abs(d) / 6) * 50)}%`,
                            [d >= 0 ? "left" : "right"]: "50%",
                          }}
                        />
                      </span>
                    </td>
                    <td className={Math.abs(d) >= 0.5 ? "strong" : "muted"}>
                      {sign(d)} dB
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <p className="muted small">
            {t("remaster.excerpt", {
              from: clock(data.fromS),
              to: clock(data.fromS + data.secs),
            })}
            {matched && mode !== "stereo"
              ? ` ${t("remaster.zonesMatched")}`
              : ""}
          </p>
        </section>
      )}
    </>
  );
}
