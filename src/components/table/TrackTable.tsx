import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, Check } from "lucide-react";
import type { SortKey, TrackRow } from "../../core/library";
import { COLUMNS, loadColumnState, saveColumnState, type ColumnState } from "./columns";

interface Props {
  ids: number[];
  rows: Map<number, TrackRow>;
  /** Lignes d'avant le dernier rafraîchissement, affichées en attendant les nouvelles. */
  staleRows: Map<number, TrackRow>;
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

export function TrackTable(props: Props) {
  const { ids, rows, onNeedRows, sort, onSortChange, selection, onSelectionChange, density, keyNotation } = props;
  const { t, i18n } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [cols, setCols] = useState<ColumnState[]>(loadColumnState);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [dragCol, setDragCol] = useState<string | null>(null);
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
  }, [first, last, ids, rows, onNeedRows]);

  const ctx = { t, locale: i18n.language, keyNotation };

  // ---------- Tri ----------
  const onHeaderClick = (e: MouseEvent, field?: string) => {
    if (!field) return;
    const existing = sort.find((s) => s.field === field);
    if (e.shiftKey) {
      // Tri sur plusieurs colonnes
      if (!existing) onSortChange([...sort, { field, desc: false }]);
      else onSortChange(sort.map((s) => (s.field === field ? { ...s, desc: !s.desc } : s)));
      return;
    }
    if (!existing || sort.length > 1) onSortChange([{ field, desc: false }]);
    else if (!existing.desc) onSortChange([{ field, desc: true }]);
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

  // ---------- Réorganisation par glisser-déposer ----------
  const dropOn = (targetId: string) => {
    if (!dragCol || dragCol === targetId) return;
    setCols((cs) => {
      const from = cs.findIndex((c) => c.id === dragCol);
      const to = cs.findIndex((c) => c.id === targetId);
      const next = [...cs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    });
    setDragCol(null);
  };

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
    selectIndex(index, e.shiftKey ? "range" : e.ctrlKey || e.metaKey ? "toggle" : "single");
  };

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
              <div
                key={state.id}
                role="columnheader"
                aria-sort={sorted ? (sorted.desc ? "descending" : "ascending") : undefined}
                className={`th ${def.sortField ? "sortable" : ""} align-${def.align ?? "left"} ${dragCol === state.id ? "dragging" : ""}`}
                draggable
                onDragStart={() => setDragCol(state.id)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => dropOn(state.id)}
                onDragEnd={() => setDragCol(null)}
                onClick={(e) => onHeaderClick(e, def.sortField)}
                title={def.sortField ? t("table.sortHint") : undefined}
              >
                <span className="th-label">{state.id === "cover" ? "" : t(`columns.${state.id}`)}</span>
                {sortIndicator(def.sortField)}
                <span className="resize-handle" onMouseDown={(e) => startResize(e, state.id)} onClick={(e) => e.stopPropagation()} aria-hidden />
              </div>
            );
          })}
        </div>

        <div style={{ height: virtualizer.getTotalSize(), width: totalWidth, position: "relative" }}>
          {items.map((vi) => {
            const id = ids[vi.index];
            const row = rows.get(id) ?? props.staleRows.get(id);
            const selected = selection.has(id);
            const cls = ["tr", selected ? "selected" : "", row && !row.online ? "offline" : "", row?.status === "missing" ? "missing" : "", vi.index % 2 ? "odd" : ""].join(" ");
            return (
              <div
                key={id}
                role="row"
                aria-rowindex={vi.index + 2}
                aria-selected={selected}
                aria-disabled={row && !row.online ? true : undefined}
                className={cls}
                style={{ transform: `translateY(${vi.start - headerHeight}px)`, height: rowHeight, gridTemplateColumns: template, width: totalWidth }}
                onMouseDown={(e) => onRowClick(e, vi.index)}
                onDoubleClick={() => props.onActivate?.(id)}
              >
                {row
                  ? visible.map(({ state, def }) => (
                      <div key={state.id} role="gridcell" className={`td align-${def.align ?? "left"}`}>
                        {def.render(row, ctx)}
                      </div>
                    ))
                  : <div className="td loading" role="gridcell" />}
              </div>
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
