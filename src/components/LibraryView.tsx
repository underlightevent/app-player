import { useTranslation } from "react-i18next";
import { FolderPlus, SearchX, Unplug } from "lucide-react";
import type { LibraryQuery, RootInfo, SortKey, TrackRow, VolumeStatus } from "../core/library";
import { TrackTable } from "./table/TrackTable";

const FORMATS = ["wav", "mp3", "flac", "aiff", "m4a"];

interface Props {
  query: LibraryQuery;
  onQueryChange: (q: LibraryQuery) => void;
  ids: number[];
  rows: Map<number, TrackRow>;
  staleRows: Map<number, TrackRow>;
  rowsVersion: number;
  needRows: (ids: number[]) => void;
  loading: boolean;
  roots: RootInfo[];
  volumes: VolumeStatus[];
  selection: Set<number>;
  onSelectionChange: (s: Set<number>, focus: number | null) => void;
  onAddFolder: () => void;
  onActivate: (id: number) => void;
  density: "compact" | "comfortable";
  keyNotation: "camelot" | "musical";
}

export function LibraryView(p: Props) {
  const { t, i18n } = useTranslation();
  const { query, onQueryChange } = p;
  const set = (patch: Partial<LibraryQuery>) => onQueryChange({ ...query, ...patch });
  const nf = new Intl.NumberFormat(i18n.language);

  // Disques utilisés par la bibliothèque mais débranchés.
  const usedVolumes = new Set(p.roots.map((r) => r.volumeId));
  const offline = p.volumes.filter((v) => usedVolumes.has(v.id) && !v.online);

  if (!p.loading && p.roots.length === 0) {
    return (
      <div className="empty">
        <FolderPlus size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
        <h1>{t("views.library.title")}</h1>
        <p>{t("views.library.text")}</p>
        <button className="btn btn-primary" onClick={p.onAddFolder}>
          <FolderPlus size={16} aria-hidden />
          {t("views.library.action")}
        </button>
        <p className="phase-note">{t("views.library.dropHint")}</p>
      </div>
    );
  }

  const filtersActive = query.formats.length > 0 || query.noCover || query.minRating > 0 || query.status != null;
  const toggleFormat = (f: string) =>
    set({ formats: query.formats.includes(f) ? query.formats.filter((x) => x !== f) : [...query.formats, f] });

  return (
    <div className="library">
      {offline.map((v) => (
        <div key={v.id} className="banner" role="status">
          <Unplug size={16} aria-hidden />
          {t("library.diskOffline", { disk: v.label })}
        </div>
      ))}

      <div className="filter-bar" role="toolbar" aria-label={t("library.filters")}>
        {query.health && (
          <button className="chip" aria-pressed onClick={() => set({ health: null })} title={t("library.clearHealth")}>
            {t(`health.keys.${query.health}`)} ×
          </button>
        )}
        {query.folder && (
          <button className="chip" aria-pressed onClick={() => set({ folder: null })} title={t("library.clearFolder")}>
            {t("library.folderFilter", { name: query.folder[1].split("/").pop() || "/" })} ×
          </button>
        )}
        {FORMATS.map((f) => (
          <button key={f} className="chip" aria-pressed={query.formats.includes(f)} onClick={() => toggleFormat(f)}>
            {f.toUpperCase()}
          </button>
        ))}
        <span className="chip-sep" />
        <button className="chip" aria-pressed={query.noCover} onClick={() => set({ noCover: !query.noCover })}>
          {t("library.noCover")}
        </button>
        <button
          className="chip"
          aria-pressed={query.status === "problems"}
          onClick={() => set({ status: query.status === "problems" ? null : "problems" })}
        >
          {t("library.problems")}
        </button>
        <label className="chip-select">
          {t("library.minRating")}
          <select value={query.minRating} onChange={(e) => set({ minRating: Number(e.target.value) })}>
            {[0, 1, 2, 3, 4, 5].map((n) => (
              <option key={n} value={n}>
                {n === 0 ? t("library.anyRating") : "★".repeat(n)}
              </option>
            ))}
          </select>
        </label>
        {filtersActive && (
          <button className="chip chip-clear" onClick={() => set({ formats: [], noCover: false, minRating: 0, status: null })}>
            {t("library.clearFilters")}
          </button>
        )}
        <span className="filter-count tabular" aria-live="polite">
          {t("library.count", { count: p.ids.length, formatted: nf.format(p.ids.length) })}
          {p.selection.size > 1 && ` · ${t("library.selected", { count: p.selection.size, formatted: nf.format(p.selection.size) })}`}
        </span>
      </div>

      {p.ids.length === 0 && !p.loading ? (
        <div className="empty">
          <SearchX size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
          <h1>{t("library.noResults")}</h1>
          <p>{t("library.noResultsHint")}</p>
        </div>
      ) : (
        <TrackTable
          ids={p.ids}
          rows={p.rows}
          staleRows={p.staleRows}
          rowsVersion={p.rowsVersion}
          onNeedRows={p.needRows}
          sort={query.sort}
          onSortChange={(sort: SortKey[]) => set({ sort })}
          selection={p.selection}
          onSelectionChange={p.onSelectionChange}
          onActivate={p.onActivate}
          density={p.density}
          keyNotation={p.keyNotation}
        />
      )}
    </div>
  );
}
