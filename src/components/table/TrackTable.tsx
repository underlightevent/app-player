import { memo, useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { consumeDragClick, startDrag, useDropTarget } from "../../core/dnd";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, Check } from "lucide-react";
import type { SortKey, TrackRow } from "../../core/library";
import { COLUMNS, loadColumnState, saveColumnState, type ColumnState, type RenderCtx } from "./columns";

interface Props {
  ids: number[];
  rows: Map<number, TrackRow>;
  /** Lignes d'avant le dernier rafraîchissement, affichées en attendant les nouvelles. */
  staleRows: Map<number, TrackRow>;
  /** Change quand de nouvelles lignes sont arrivées. */
  rowsVersion: number;
  /** Demande le chargement des lignes visibles. */
  onNeedRows: (ids: number[]) => void;
  sort: SortKey[];
  onSortChange: (sort: SortKey[]) => void;
  selection: Set<number>;
  onSelectionChange: (sel: Set<number>, focusId: number | null) => void;
  onActivate?: (id: number) => void;
  density: "compact" | "comfortable";
  keyNotation: "camelot" | "musical";
}

/** En-tête de colonne : clic pour trier, glisser pour réordonner, poignée pour la largeur. */
function HeaderCell(p: {
  id: string;
  label: string;
  ariaSort?: "ascending" | "descending";
  className: string;
  title?: string;
  onClick: (e: MouseEvent) => void;
  onDropColumn: (from: string) => void;
  indicator: ReactNode;
  onResizeStart: (e: MouseEvent) => void;
}) {
  const ref = useDropTarget<HTMLDivElement>("column", (payload) => {
    if (payload.kind === "column") p.onDropColumn(payload.id);
  });
  return (
    <div
      ref={ref}
      role="columnheader"
      aria-sort={p.ariaSort}
      className={p.className}
      onMouseDown={(e) => startDrag(e, () => ({ kind: "column", id: p.id }), () => p.label || "—")}
      onClick={(e) => {
        if (!consumeDragClick()) p.onClick(e);
      }}
      title={p.title}
    >
      <span className="th-label">{p.label}</span>
      {p.indicator}
      <span className="resize-handle" onMouseDown={(e) => { e.stopPropagation(); p.onResizeStart(e); }} onClick={(e) => e.stopPropagation()} aria-hidden />
    </div>
  );
}

interface RowProps {
  id: number;
  index: number;
  top: number;
  height: number;
  row: TrackRow | undefined;
  selected: boolean;
  template: string;
  width: number;
  visible: { state: ColumnState; def: (typeof COLUMNS)[number] }[];
  ctx: RenderCtx;
  onDown: (e: MouseEvent, index: number) => void;
  onActivate: (id: number) => void;
}

/** Une ligne du tableau. Mémorisée : le défilement ne redessine pas les lignes inchangées. */
const Row = memo(function Row(p: RowProps) {
  const { row } = p;
  const cls = ["tr", p.selected ? "selected" : "", row && !row.online ? "offline" : "", row?.status === "missing" ? "missing" : "", p.index % 2 ? "odd" : ""].join(" ");
  return (
    <div
      role="row"
      aria-rowindex={p.index + 2}
      aria-selected={p.selected}
      aria-disabled={row && !row.online ? true : undefined}
      className={cls}
      style={{ transform: `translateY(${p.top}px)`, height: p.height, gridTemplateColumns: p.template, width: p.width }}
      onMouseDown={(e) => p.onDown(e, p.index)}
      onDoubleClick={() => p.onActivate(p.id)}
    >
      {row ? (
        p.visible.map(({ state, def }) => (
          <div key={state.id} role="gridcell" className={`td align-${def.align ?? "left"}`}>
            {def.render(row, p.ctx)}
          </div>
        ))
      ) : (
        <div className="td loading" role="gridcell" />
      )}
    </div>
  );
});

export function TrackTable(props: Props) {
  const { ids, rows, onNeedRows, sort, onSortChange, selection, onSelectionChange, density, keyNotation } = props;
  const { t, i18n } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [cols, setCols] = useState<ColumnState[]>(loadColumnState);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const anchor = useRef<number | null>(null);
  const focus = useRef<number | null>(null);
  const rowHeight = density === "compact" ? 28 : 36;

  useEffect(() => saveColumnState(cols), [cols]);

  const visible = useMemo(
    () => cols.filter((c) => c.visible).map((c) => ({ state: c, def: COLUMNS.find((d) => d.id === c.id)! })),
    [cols],
  );
  const template = visible.map((c) => `${c.state.width}px`).join(" ");
  const totalWidth = visible.reduce((s, c) => s + c.state.width, 0);

  // L'en-tête collant occupe le haut de la zone de défilement.
  const headerHeight = 32;
  const virtualizer = useVirtualizer({
    count: ids.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 15,
    scrollMargin: headerHeight,
    scrollPaddingStart: headerHeight,
  });
  useEffect(() => virtualizer.measure(), [rowHeight, virtualizer]);

  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;

  // Charger les lignes visibles (avec une marge) qui ne sont pas encore connues.
  useEffect(() => {
    if (ids.length === 0) return;
    const from = Math.max(0, first - 60);
    const to = Math.min(ids.length, last + 60);
    const missing = ids.slice(from, to).filter((id) => !rows.has(id));
    if (missing.length) onNeedRows(missing);
  }, [first, last, ids, rows, onNeedRows, props.rowsVersion]);

  const ctx = useMemo(() => ({ t, locale: i18n.language, keyNotation }), [t, i18n.language, keyNotation]);
  // Gestionnaires stables : une ligne n'est redessinée que si son contenu change.
  const handlers = useRef({ down: (_e: MouseEvent, _i: number) => {}, activate: (_id: number) => {} });
  const rowDown = useCallback((e: MouseEvent, i: number) => handlers.current.down(e, i), []);
  const rowActivate = useCallback((id: number) => handlers.current.activate(id), []);

  // ---------- Tri ----------
  const DESC_FIRST = ["rating"];
  const onHeaderClick = (e: MouseEvent, field?: string) => {
    if (!field) return;
    const existing = sort.find((s) => s.field === field);
    if (e.shiftKey) {
      // Tri sur plusieurs colonnes
      if (!existing) onSortChange([...sort, { field, desc: false }]);
      else onSortChange(sort.map((s) => (s.field === field ? { ...s, desc: !s.desc } : s)));
      return;
    }
    // Note : les mieux notés d'abord au premier clic (puis l'inverse, puis sans tri).
    const first = DESC_FIRST.includes(field);
    if (!existing || sort.length > 1) onSortChange([{ field, desc: first }]);
    else if (existing.desc === first) onSortChange([{ field, desc: !first }]);
    else onSortChange([]);
  };

  // ---------- Redimensionnement ----------
  const startResize = (e: MouseEvent, id: string) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = cols.find((c) => c.id === id)!.width;
    const min = COLUMNS.find((c) => c.id === id)?.minWidth ?? 40;
    const move = (ev: globalThis.MouseEvent) =>
      setCols((cs) => cs.map((c) => (c.id === id ? { ...c, width: Math.max(min, startW + ev.clientX - startX) } : c)));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  // ---------- Réorganisation des colonnes par glisser-déposer ----------
  const dropOn = (dragged: string, targetId: string) => {
    if (dragged === targetId) return;
    setCols((cs) => {
      const from = cs.findIndex((c) => c.id === dragged);
      const to = cs.findIndex((c) => c.id === targetId);
      if (from < 0 || to < 0) return cs;
      const next = [...cs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    });
  };

  // Glisser des morceaux (vers un dossier) : la sélection, ou la ligne saisie.
  const selectionRef = useRef(selection);
  selectionRef.current = selection;

  // ---------- Sélection ----------
  const selectIndex = useCallback(
    (index: number, mode: "single" | "toggle" | "range") => {
      const id = ids[index];
      if (id == null) return;
      let next: Set<number>;
      if (mode === "toggle") {
        next = new Set(selection);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        anchor.current = index;
      } else if (mode === "range" && anchor.current != null) {
        const [a, b] = [anchor.current, index].sort((x, y) => x - y);
        next = new Set(ids.slice(a, b + 1));
      } else {
        next = new Set([id]);
        anchor.current = index;
      }
      focus.current = index;
      onSelectionChange(next, id);
    },
    [ids, selection, onSelectionChange],
  );

  const onRowClick = (e: MouseEvent, index: number) => {
    scrollRef.current?.focus({ preventScroll: true });
    const id = ids[index];
    // Un clic sur une ligne déjà sélectionnée garde la sélection (pour pouvoir la faire glisser) ;
    // relâché sans glisser, il ne sélectionne plus que cette ligne.
    const keep = selection.has(id) && !e.shiftKey && !e.ctrlKey && !e.metaKey && selection.size > 1;
    if (!keep) {
      selectIndex(index, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "single");
    }
    if (!e.shiftKey && !e.ctrlKey && !e.metaKey) {
      startDrag(
        e,
        () => {
          const sel = selectionRef.current;
          return { kind: "tracks", ids: sel.has(id) ? [...sel] : [id] };
        },
        () => {
          const sel = selectionRef.current;
          const n = sel.has(id) ? sel.size : 1;
          return t("table.dragTracks", { count: n });
        },
        keep ? () => selectIndex(index, "single") : undefined,
      );
    }
  };

  handlers.current = { down: onRowClick, activate: (id) => props.onActivate?.(id) };

  const onKeyDown = (e: KeyboardEvent) => {
    const current = focus.current ?? -1;
    const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 400) / rowHeight) - 1);
    let target: number | null = null;
    if (e.key === "ArrowDown") target = Math.min(ids.length - 1, current + 1);
    else if (e.key === "ArrowUp") target = Math.max(0, current - 1);
    else if (e.key === "PageDown") target = Math.min(ids.length - 1, current + page);
    else if (e.key === "PageUp") target = Math.max(0, current - page);
    else if (e.key === "Home") target = 0;
    else if (e.key === "End") target = ids.length - 1;
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      onSelectionChange(new Set(ids), ids[current] ?? ids[0] ?? null);
      return;
    } else if (e.key === "Enter" && current >= 0) {
      props.onActivate?.(ids[current]);
      return;
    }
    if (target == null || ids.length === 0) return;
    e.preventDefault();
    selectIndex(target, e.shiftKey ? "range" : "single");
    virtualizer.scrollToIndex(target, { align: "auto" });
  };

  const sortIndicator = (field?: string) => {
    if (!field) return null;
    const i = sort.findIndex((s) => s.field === field);
    if (i < 0) return null;
    const Icon = sort[i].desc ? ArrowDown : ArrowUp;
    return (
      <span className="sort-indicator">
        <Icon size={12} aria-hidden />
        {sort.length > 1 && <sup>{i + 1}</sup>}
      </span>
    );
  };

  return (
    <div className="table-wrap">
      <div
        ref={scrollRef}
        className={`table-scroll density-${density}`}
        role="grid"
        aria-rowcount={ids.length + 1}
        aria-multiselectable
        aria-label={t("table.label")}
        tabIndex={0}
        onKeyDown={onKeyDown}
      >
        <div className="table-header" role="row" style={{ gridTemplateColumns: template, width: totalWidth }} onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY }); }}>
          {visible.map(({ state, def }) => {
            const sorted = sort.find((s) => s.field === def.sortField);
            return (
              <HeaderCell
                key={state.id}
                id={state.id}
                label={state.id === "cover" ? "" : t(`columns.${state.id}`)}
                ariaSort={sorted ? (sorted.desc ? "descending" : "ascending") : undefined}
                className={`th ${def.sortField ? "sortable" : ""} align-${def.align ?? "left"}`}
                title={def.sortField ? t("table.sortHint") : undefined}
                onClick={(e) => onHeaderClick(e, def.sortField)}
                onDropColumn={(from) => dropOn(from, state.id)}
                indicator={sortIndicator(def.sortField)}
                onResizeStart={(e) => startResize(e, state.id)}
              />
            );
          })}
        </div>

        <div style={{ height: virtualizer.getTotalSize(), width: totalWidth, position: "relative" }}>
          {items.map((vi) => {
            const id = ids[vi.index];
            return (
              <Row
                key={id}
                id={id}
                index={vi.index}
                top={vi.start - headerHeight}
                height={rowHeight}
                row={rows.get(id) ?? props.staleRows.get(id)}
                selected={selection.has(id)}
                template={template}
                width={totalWidth}
                visible={visible}
                ctx={ctx}
                onDown={rowDown}
                onActivate={rowActivate}
              />
            );
          })}
        </div>
      </div>

      {menu && (
        <>
          <div className="menu-backdrop" onClick={() => setMenu(null)} onContextMenu={(e) => { e.preventDefault(); setMenu(null); }} />
          <div className="menu" style={{ left: menu.x, top: menu.y }} role="menu" aria-label={t("table.columnsMenu")}>
            {cols.map((c) => (
              <button
                key={c.id}
                role="menuitemcheckbox"
                aria-checked={c.visible}
                className="menu-item"
                onClick={() => setCols((cs) => cs.map((x) => (x.id === c.id ? { ...x, visible: !x.visible } : x)))}
              >
                <span className="menu-check">{c.visible && <Check size={14} aria-hidden />}</span>
                {t(`columns.${c.id}`)}
              </button>
            ))}
            <div className="menu-sep" />
            <button className="menu-item" onClick={() => { setCols(COLUMNS.map((c) => ({ id: c.id, width: c.width, visible: c.defaultVisible }))); setMenu(null); }}>
              <span className="menu-check" />
              {t("table.resetColumns")}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
