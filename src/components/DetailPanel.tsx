import { useTranslation } from "react-i18next";
import { Disc3, Info } from "lucide-react";
import type { TrackRow } from "../core/library";
import { formatBpm, formatDuration, formatKey, formatQuality, formatSize } from "../core/format";
import { Stars } from "./table/columns";

interface Props {
  selected: TrackRow[];
  selectionSize: number;
  keyNotation: "camelot" | "musical";
}

export function DetailPanel({ selected, selectionSize, keyNotation }: Props) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;

  if (selectionSize === 0) {
    return (
      <aside className="detail" aria-label={t("detail.title")}>
        <h2 className="detail-title">{t("detail.title")}</h2>
        <div className="cover-placeholder" aria-hidden>
          <Disc3 size={48} strokeWidth={1.25} />
        </div>
        <p>{t("detail.empty")}</p>
        <p className="muted">{t("detail.hint")}</p>
      </aside>
    );
  }

  if (selectionSize === 1 && !selected[0]) {
    return <aside className="detail" aria-label={t("detail.title")} aria-busy />;
  }

  if (selectionSize > 1) {
    return (
      <aside className="detail" aria-label={t("detail.title")}>
        <h2 className="detail-title">{t("detail.title")}</h2>
        <div className="cover-placeholder" aria-hidden>
          <Disc3 size={48} strokeWidth={1.25} />
        </div>
        <p>{t("detail.multiple", { count: selectionSize })}</p>
        <p className="muted">{t("detail.multipleHint")}</p>
      </aside>
    );
  }

  const r = selected[0];
  const field = (label: string, value: React.ReactNode) =>
    value === "" || value == null ? null : (
      <div className="field">
        <dt>{label}</dt>
        <dd>{value}</dd>
      </div>
    );

  return (
    <aside className="detail" aria-label={t("detail.title")}>
      <div className="cover-placeholder" aria-hidden>
        <Disc3 size={48} strokeWidth={1.25} />
      </div>
      <h2 className="detail-track-title">
        {r.title ?? r.fileName}
        {r.version && <span className="detail-version"> ({r.version})</span>}
      </h2>
      <div className="detail-artists">
        {r.artists.map((a, i) => (
          <span key={a + i} className={`artist-tag ${i === 0 ? "main" : ""}`} title={i === 0 ? t("detail.mainArtist") : undefined}>
            {i === 0 && "★ "}
            {a}
          </span>
        ))}
      </div>
      <Stars value={r.rating} />

      {r.format === "wav" && (
        <p className="note-info">
          <Info size={14} aria-hidden /> {t("badges.wav")}
        </p>
      )}
      {!r.online && <p className="note-info">{t("badges.offline", { disk: r.volumeLabel })}</p>}
      {r.status === "unreadable" && <p className="note-info text-error">{r.error ?? t("badges.unreadable")}</p>}

      <dl className="fields">
        {field(t("fields.album"), r.album)}
        {field(t("fields.year"), r.year)}
        {field(t("fields.genre"), r.genre)}
        {field(t("fields.bpm"), formatBpm(r.bpm, locale))}
        {field(t("fields.key"), r.key ? `${formatKey(r.key, keyNotation)} (${keyNotation === "camelot" ? r.key : formatKey(r.key, "camelot")})` : "")}
      </dl>

      <h3 className="detail-section">{t("detail.technical")}</h3>
      <dl className="fields tabular">
        {field(t("detail.quality"), formatQuality(r, locale))}
        {field(t("detail.duration"), formatDuration(r.durationMs))}
        {field(t("detail.size"), formatSize(r.size, locale))}
        {field(t("detail.channels"), r.channels)}
        {field(t("detail.cover"), r.coverCount > 0 ? `${r.coverWidth ?? "?"} px${r.coverCount > 1 ? ` · ${t("detail.coverCount", { count: r.coverCount })}` : ""}` : t("badges.noCover"))}
        {field(t("detail.file"), <span className="path">{r.path ?? `${r.volumeLabel}/${r.relPath}`}</span>)}
      </dl>
    </aside>
  );
}
