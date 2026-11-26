import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Globe, ListTodo, Loader2, Moon, MoreHorizontal, PanelRight, PenLine, Search, Sun, X } from "lucide-react";
import type { ResolvedTheme } from "../core/theme";
import type { TaskInfo } from "../core/library";
import { TasksPanel } from "./TasksPanel";

export interface MoreAction {
  id: string;
  label: string;
  disabled?: boolean;
  run: () => void;
}

interface Props {
  onSearchOnline: () => void;
  /** Recherches en attente dans la file, et coupure détectée par le cœur. */
  onlineQueued: number;
  onlineCoreOffline: boolean;
  onRename: () => void;
  renameDisabled: boolean;
  moreActions: MoreAction[];
  search: string;
  onSearchChange: (s: string) => void;
  resolvedTheme: ResolvedTheme;
  onToggleTheme: () => void;
  detailOpen: boolean;
  onToggleDetail: () => void;
  tasks: TaskInfo[];
  runningTasks: number;
  modKey: string;
}

function useOnline() {
  const [online, setOnline] = useState(navigator.onLine);
  useEffect(() => {
    const on = () => setOnline(true);
    const off = () => setOnline(false);
    window.addEventListener("online", on);
    window.addEventListener("offline", off);
    return () => {
      window.removeEventListener("online", on);
      window.removeEventListener("offline", off);
    };
  }, []);
  return online;
}

export function TopBar(p: Props) {
  const { t } = useTranslation();
  const online = useOnline();
  const [tasksOpen, setTasksOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const themeLabel = p.resolvedTheme === "dark" ? t("topbar.themeToLight") : t("topbar.themeToDark");

  // Ctrl+F (Cmd+F) : rechercher dans la bibliothèque.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <header className="topbar" data-tauri-drag-region>
      <label className="search">
        <Search size={16} aria-hidden />
        <span className="visually-hidden">{t("topbar.searchPlaceholder")}</span>
        <input
          ref={searchRef}
          type="text"
          spellCheck={false}
          placeholder={`${t("topbar.searchPlaceholder")} (${p.modKey}+F)`}
          value={p.search}
          onChange={(e) => p.onSearchChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              p.onSearchChange("");
              (e.target as HTMLInputElement).blur();
            }
          }}
        />
        {p.search && (
          <button className="icon-btn small" onClick={() => p.onSearchChange("")} aria-label={t("topbar.clearSearch")}>
            <X size={14} aria-hidden />
          </button>
        )}
      </label>

      <button className="btn" onClick={p.onSearchOnline} disabled={p.renameDisabled} title={`${t("topbar.searchOnline")} (${p.modKey}+L)`}>
        <Globe size={16} aria-hidden />
        {t("topbar.searchOnline")}
      </button>
      <button className="btn" onClick={p.onRename} disabled={p.renameDisabled} title={`${t("topbar.rename")} (${p.modKey}+R)`}>
        <PenLine size={16} aria-hidden />
        {t("topbar.rename")}
      </button>
      <div className="popover-anchor">
        <button className="icon-btn" onClick={() => setMoreOpen((o) => !o)} aria-label={t("topbar.more")} aria-expanded={moreOpen} title={t("topbar.more")}>
          <MoreHorizontal size={20} aria-hidden />
        </button>
        {moreOpen && (
          <>
            <div className="menu-backdrop" onClick={() => setMoreOpen(false)} />
            <div className="menu popover-menu" role="menu">
              {p.moreActions.map((a) => (
                <button
                  key={a.id}
                  role="menuitem"
                  className="menu-item"
                  disabled={a.disabled}
                  onClick={() => {
                    setMoreOpen(false);
                    a.run();
                  }}
                >
                  {a.label}
                </button>
              ))}
            </div>
          </>
        )}
      </div>

      <div className="topbar-spacer drag-region" data-tauri-drag-region />

      <span className="status-pill" role="status" title={p.onlineQueued ? t("topbar.queueHint") : undefined}>
        <span className={`status-dot ${online && !p.onlineCoreOffline ? "online" : "offline"}`} aria-hidden />
        {online && !p.onlineCoreOffline ? t("topbar.online") : t("topbar.offline")}
        {p.onlineQueued > 0 && <span className="tabular"> · {t("topbar.queued", { count: p.onlineQueued })}</span>}
      </span>

      <div className="popover-anchor">
        <button
          className={`icon-btn ${p.runningTasks ? "active" : ""}`}
          onClick={() => setTasksOpen((o) => !o)}
          aria-label={t("topbar.tasks")}
          aria-expanded={tasksOpen}
          title={t("topbar.tasks")}
        >
          {p.runningTasks ? <Loader2 size={20} className="spin" aria-hidden /> : <ListTodo size={20} aria-hidden />}
        </button>
        {tasksOpen && <TasksPanel list={p.tasks} onClose={() => setTasksOpen(false)} />}
      </div>
      <button className="icon-btn" onClick={p.onToggleTheme} aria-label={themeLabel} title={themeLabel}>
        {p.resolvedTheme === "dark" ? <Sun size={20} aria-hidden /> : <Moon size={20} aria-hidden />}
      </button>
      <button
        className="icon-btn"
        onClick={p.onToggleDetail}
        aria-pressed={p.detailOpen}
        aria-label={t("topbar.toggleDetail")}
        title={`${t("topbar.toggleDetail")} (${p.modKey}+I)`}
      >
        <PanelRight size={20} aria-hidden />
      </button>
    </header>
  );
}
