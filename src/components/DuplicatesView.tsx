import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { BadgeCheck, CopyX, Image as ImageIcon, Pause, Play, Search, SplitSquareHorizontal, TriangleAlert } from "lucide-react";
import { onEvent } from "../core/library";
import type { EditReport } from "../core/edit";
import { trackCover } from "../core/edit";
import {
  duplicatesFind,
  duplicatesList,
  duplicatesNotDuplicates,
  duplicatesPlan,
  duplicatesResolve,
  duplicatesSetKeeper,
  type DuplicateGroup,
  type ResolvePlan,
  type ResolveRequest,
} from "../core/quality";
import { formatDuration, formatQuality, formatSize } from "../core/format";
import { Modal } from "./Modal";

interface Props {
  playingId: number | null;
  playing: boolean;
  onPlay: (id: number) => void;
  onToggle: () => void;
  onDone: (r: EditReport, key: string) => void;
  onToast: (text: string) => void;
}

function Cover({ id, has }: { id: number; has: boolean }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    if (has) void trackCover(id, 160).then((s) => alive && setSrc(s)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [id, has]);
  return src ? <img className="dup-cover" src={src} alt="" /> : <div className="dup-cover dup-cover-empty"><ImageIcon size={20} aria-hidden /></div>;
}

/** Écran Doublons : un groupe par morceau, fichiers côte à côte, choix du fichier à garder. */
export function DuplicatesView({ playingId, playing, onPlay, onToggle, onDone, onToast }: Props) {
  const { t, i18n } = useTranslation();
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const [plans, setPlans] = useState<{ plans: ResolvePlan[]; requests: ResolveRequest[] } | null>(null);
  const [transfer, setTransfer] = useState(true);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => setGroups(await duplicatesList()), []);
  useEffect(() => {
    void load();
    return onEvent("library:changed", () => void load());
  }, [load]);

  const search = async () => {
    const started = await duplicatesFind();
    onToast(started ? t("duplicates.searching") : t("duplicates.alreadySearching"));
  };

  const keeperOf = (g: DuplicateGroup) => g.keeperId ?? g.members[0]?.track.id;

  const preview = async (gs: DuplicateGroup[]) => {
    const requests = gs.map((g) => ({ groupId: g.id, keeperId: keeperOf(g), transfer }));
    setPlans({ plans: await duplicatesPlan(requests), requests });
  };

  const apply = async () => {
    if (!plans) return;
    setBusy(true);
    try {
      const requests = plans.requests.map((r) => ({ ...r, transfer }));
      onDone(await duplicatesResolve(requests), "duplicates.done");
      setPlans(null);
      await load();
    } finally {
      setBusy(false);
    }
  };

  const nf = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 });

  return (
    <div className="settings duplicates">
      <h1>{t("nav.duplicates")}</h1>
      <p className="hint">{t("duplicates.intro")}</p>
      <div className="folder-actions">
        <button className="btn" onClick={() => void search()}>
          <Search size={16} aria-hidden /> {t("duplicates.search")}
        </button>
        <button className="btn btn-primary" disabled={!groups?.length} onClick={() => groups && void preview(groups)}>
          {t("duplicates.resolveAll", { count: groups?.length ?? 0 })}
        </button>
      </div>

      {groups && groups.length === 0 && (
        <div className="empty small-empty">
          <CopyX size={36} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <p>{t("duplicates.none")}</p>
        </div>
      )}

      {groups?.map((g) => {
        const keeper = keeperOf(g);
        return (
          <section key={g.id} className="dup-group" aria-label={t("duplicates.group")}>
            <div className="dup-head">
              <strong>
                {g.members[0]?.track.mainArtist ?? ""} – {g.members[0]?.track.title ?? g.members[0]?.track.fileName}
              </strong>
              <span className={`badge ${g.kind === "exact" ? "badge-info" : ""}`}>{t(`duplicates.kind.${g.kind}`)}</span>
              <span className="spacer" />
              <button className="btn btn-small" onClick={() => void duplicatesNotDuplicates(g.id).then(load)}>
                <SplitSquareHorizontal size={14} aria-hidden /> {t("duplicates.notDuplicates")}
              </button>
              <button className="btn btn-small btn-primary" onClick={() => void preview([g])}>
                {t("duplicates.resolve")}
              </button>
            </div>
            <div className="dup-members">
              {g.members.map((m) => {
                const r = m.track;
                const isKeeper = r.id === keeper;
                const isPlaying = playingId === r.id && playing;
                return (
                  <div key={r.id} className={`dup-card ${isKeeper ? "keeper" : ""}`}>
                    <div className="dup-card-top">
                      <Cover id={r.id} has={r.coverCount > 0} />
                      <div className="dup-card-title">
                        <div className="dup-format">
                          <span className="badge">{r.format.toUpperCase()}</span> {formatQuality(r, i18n.language)}
                        </div>
                        {isKeeper ? (
                          <span className="badge badge-keep"><BadgeCheck size={14} aria-hidden /> {t("duplicates.keep")}</span>
                        ) : (
                          <button className="btn btn-small" onClick={() => void duplicatesSetKeeper(g.id, r.id).then(load)}>
                            {t("duplicates.makeKeeper")}
                          </button>
                        )}
                      </div>
                      <button
                        className="icon-btn"
                        aria-label={isPlaying ? t("player.pause") : t("player.play")}
                        onClick={() => (playingId === r.id ? onToggle() : onPlay(r.id))}
                        disabled={!r.online}
                      >
                        {isPlaying ? <Pause size={18} aria-hidden /> : <Play size={18} aria-hidden />}
                      </button>
                    </div>
                    <dl className="dup-facts">
                      <dt>{t("columns.duration")}</dt>
                      <dd className="tabular">{formatDuration(r.durationMs)}</dd>
                      <dt>{t("duplicates.size")}</dt>
                      <dd className="tabular">{formatSize(r.size, i18n.language)}</dd>
                      <dt>{t("duplicates.tags")}</dt>
                      <dd>
                        {[r.album, r.year, r.genre].filter(Boolean).join(" · ") || "—"}
                        {r.rating > 0 && ` · ${"★".repeat(r.rating)}`}
                      </dd>
                      <dt>{t("duplicates.path")}</dt>
                      <dd className="dup-path" title={r.path ?? r.relPath}>{r.relPath}</dd>
                      {m.similarity != null && g.kind === "similar" && (
                        <>
                          <dt>{t("duplicates.similarity")}</dt>
                          <dd className="tabular">{nf.format(m.similarity * 100)} %</dd>
                        </>
                      )}
                    </dl>
                    {m.suspect && (
                      <p className="dup-warn">
                        <TriangleAlert size={14} aria-hidden /> {t("duplicates.suspect", { khz: m.cutoffHz ? (m.cutoffHz / 1000).toFixed(1) : "?" })}
                      </p>
                    )}
                    {g.suggestedId != null && g.suggestedId !== keeper && g.suggestedId === r.id && (
                      <p className="hint small">{t("duplicates.suggested")}</p>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        );
      })}

      {plans && (
        <Modal
          title={t("duplicates.previewTitle", { count: plans.plans.length })}
          onClose={() => setPlans(null)}
          wide
          footer={
            <>
              <button className="btn" onClick={() => setPlans(null)}>{t("common.cancel")}</button>
              <button className="btn btn-primary" disabled={busy} onClick={() => void apply()}>
                {t("duplicates.confirm", { count: plans.plans.reduce((n, p) => n + p.discard.length, 0) })}
              </button>
            </>
          }
        >
          <label className="check">
            <input type="checkbox" checked={transfer} onChange={(e) => setTransfer(e.target.checked)} />
            {t("duplicates.transfer")}
          </label>
          <p className="hint small">{t("duplicates.previewHint")}</p>
          <ul className="dup-plan">
            {plans.plans.map((p) => (
              <li key={p.groupId}>
                <div>
                  <BadgeCheck size={14} aria-hidden /> <strong>{p.keeperFile}</strong>
                </div>
                {transfer && p.transfers.length > 0 && (
                  <ul className="dup-transfers">
                    {p.transfers.map((tr) => (
                      <li key={tr.field}>
                        {t("duplicates.transferLine", { field: t(`fields.${tr.field}`, { defaultValue: tr.field }), value: tr.value, from: tr.fromFile })}
                      </li>
                    ))}
                  </ul>
                )}
                <ul className="dup-discard">
                  {p.discard.map(([id, name]) => (
                    <li key={id}>{t("duplicates.toQuarantine", { name })}</li>
                  ))}
                </ul>
                {p.blockers.map((b) => (
                  <p key={b} className="dup-warn"><TriangleAlert size={14} aria-hidden /> {b}</p>
                ))}
              </li>
            ))}
          </ul>
        </Modal>
      )}
    </div>
  );
}
