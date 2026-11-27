import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BarChart3 } from "lucide-react";
import { onEvent } from "../core/library";
import { formatSize } from "../core/format";
import { statsGet, type Count, type Stats } from "../core/djtools";

/** Barres horizontales simples (valeurs relatives à la plus grande). */
function Bars({ items, label, showSize, locale }: { items: Count[]; label?: (c: Count) => string; showSize?: boolean; locale: string }) {
  const max = Math.max(1, ...items.map((i) => i.count));
  const nf = new Intl.NumberFormat(locale);
  return (
    <ul className="bars">
      {items.map((c, i) => (
        <li key={`${c.label}-${i}`} className="bar-row">
          <span className="bar-label" title={label ? label(c) : c.label}>{label ? label(c) : c.label || "—"}</span>
          <span className="bar-track" aria-hidden>
            <span className="bar-fill" style={{ width: `${(c.count / max) * 100}%` }} />
          </span>
          <span className="bar-value tabular">
            {nf.format(c.count)}
            {showSize && c.size > 0 && <span className="muted"> · {formatSize(c.size, locale)}</span>}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** Écran Statistiques de la bibliothèque. */
export function StatsView() {
  const { t, i18n } = useTranslation();
  const [s, setS] = useState<Stats | null>(null);
  useEffect(() => {
    const load = () => void statsGet().then(setS).catch(() => {});
    load();
    return onEvent("library:changed", load);
  }, []);
  const nf = new Intl.NumberFormat(i18n.language);
  if (!s) return <div className="settings stats"><h1>{t("nav.stats")}</h1></div>;
  const hours = Math.round(s.durationMs / 3_600_000);
  const month = (m: string) => {
    const [y, mo] = m.split("-").map(Number);
    return Number.isFinite(y) && Number.isFinite(mo) ? new Date(y, mo - 1, 1).toLocaleDateString(i18n.language, { month: "short", year: "numeric" }) : m;
  };
  return (
    <div className="settings stats">
      <h1>{t("nav.stats")}</h1>
      <div className="stat-cards">
        <div className="stat-card"><span className="stat-num tabular">{nf.format(s.tracks)}</span><span className="muted">{t("stats.tracks")}</span></div>
        <div className="stat-card"><span className="stat-num tabular">{formatSize(s.size, i18n.language)}</span><span className="muted">{t("stats.size")}</span></div>
        <div className="stat-card"><span className="stat-num tabular">{nf.format(hours)} h</span><span className="muted">{t("stats.duration")}</span></div>
      </div>
      {s.tracks === 0 ? (
        <div className="empty small-empty">
          <BarChart3 size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("stats.empty")}</p>
        </div>
      ) : (
        <div className="stat-grid">
          <section><h2>{t("stats.genres")}</h2><Bars items={s.genres} locale={i18n.language} /></section>
          <section><h2>{t("stats.years")}</h2><Bars items={s.years} label={(c) => `${c.label}s`} locale={i18n.language} /></section>
          <section><h2>{t("stats.formats")}</h2><Bars items={s.formats} showSize locale={i18n.language} /></section>
          <section><h2>{t("stats.quality")}</h2><Bars items={s.quality} label={(c) => t(`stats.q.${c.label}`, { defaultValue: c.label })} locale={i18n.language} /></section>
          <section><h2>{t("stats.keys")}</h2><Bars items={s.keys} locale={i18n.language} /></section>
          <section><h2>{t("stats.bpm")}</h2><Bars items={s.bpmRanges} label={(c) => `${c.label}–${Number(c.label) + 9}`} locale={i18n.language} /></section>
          <section><h2>{t("stats.disks")}</h2><Bars items={s.disks} showSize locale={i18n.language} /></section>
          <section><h2>{t("stats.added")}</h2><Bars items={s.addedByMonth} label={(c) => month(c.label)} locale={i18n.language} /></section>
          <section>
            <h2>{t("stats.mostPlayed")}</h2>
            {s.mostPlayed.length ? <Bars items={s.mostPlayed} locale={i18n.language} /> : <p className="muted small">{t("stats.noHistory")}</p>}
          </section>
        </div>
      )}
    </div>
  );
}
