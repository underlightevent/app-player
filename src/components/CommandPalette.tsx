import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Music, Search, Zap } from "lucide-react";
import { emptyQuery, libraryQuery, libraryRows, type TrackRow } from "../core/library";

export interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

function normalize(s: string) {
  return s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** Palette de commandes (Ctrl+K / Cmd+K) : toutes les actions et tous les morceaux. */
export function CommandPalette({ actions, onClose, onTrack }: { actions: PaletteAction[]; onClose: () => void; onTrack: (r: TrackRow) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [tracks, setTracks] = useState<TrackRow[]>([]);
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => input.current?.focus(), []);

  const shownActions = useMemo(() => {
    const q = normalize(text.trim());
    return q ? actions.filter((a) => normalize(a.label).includes(q)) : actions;
  }, [actions, text]);

  useEffect(() => {
    const q = text.trim();
    if (q.length < 2) {
      setTracks([]);
      return;
    }
    let stale = false;
    const h = window.setTimeout(async () => {
      const ids = await libraryQuery({ ...emptyQuery, search: q });
      const rows = await libraryRows(ids.slice(0, 20));
      if (!stale) setTracks(rows);
    }, 120);
    return () => {
      stale = true;
      window.clearTimeout(h);
    };
  }, [text]);

  const items: { key: string; label: string; sub?: string; icon: "action" | "track"; run: () => void }[] = [
    ...shownActions.map((a) => ({ key: `a-${a.id}`, label: a.label, sub: a.hint, icon: "action" as const, run: a.run })),
    ...tracks.map((r) => ({
      key: `t-${r.id}`,
      label: `${r.artists[0] ?? ""} – ${r.title ?? r.fileName}${r.version ? ` (${r.version})` : ""}`,
      sub: r.album ?? r.fileName,
      icon: "track" as const,
      run: () => onTrack(r),
    })),
  ];

  useEffect(() => setIndex(0), [text]);

  const exec = (i: number) => {
    const it = items[i];
    if (!it) return;
    onClose();
    it.run();
  };

  return (
    <>
      <div className="menu-backdrop dim" onClick={onClose} />
      <div className="palette" role="dialog" aria-modal aria-label={t("palette.title")}>
        <div className="palette-input">
          <Search size={18} aria-hidden />
          <input
            ref={input}
            value={text}
            placeholder={t("palette.placeholder")}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex((i) => Math.min(items.length - 1, i + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => Math.max(0, i - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                exec(index);
              } else if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              }
            }}
            aria-activedescendant={items[index]?.key}
          />
        </div>
        <ul className="palette-list" role="listbox">
          {items.length === 0 && <li className="muted palette-empty">{t("palette.nothing")}</li>}
          {items.map((it, i) => (
            <li key={it.key} id={it.key} role="option" aria-selected={i === index}>
              <button className={`palette-item ${i === index ? "on" : ""}`} onMouseEnter={() => setIndex(i)} onClick={() => exec(i)}>
                {it.icon === "action" ? <Zap size={16} aria-hidden /> : <Music size={16} aria-hidden />}
                <span className="palette-label">{it.label}</span>
                {it.sub && <span className="muted small palette-sub">{it.sub}</span>}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </>
  );
}
