import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Globe, ListTodo, Moon, PanelRight, PenLine, Search, Sun } from "lucide-react";
import type { ResolvedTheme } from "../core/theme";

interface Props {
  resolvedTheme: ResolvedTheme;
  onToggleTheme: () => void;
  detailOpen: boolean;
  onToggleDetail: () => void;
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

export function TopBar({ resolvedTheme, onToggleTheme, detailOpen, onToggleDetail }: Props) {
  const { t } = useTranslation();
  const online = useOnline();
  const themeLabel = resolvedTheme === "dark" ? t("topbar.themeToLight") : t("topbar.themeToDark");

  return (
    <header className="topbar" data-tauri-drag-region>
      <label className="search">
        <Search size={16} aria-hidden />
        <span className="visually-hidden">{t("topbar.searchPlaceholder")}</span>
        <input type="search" placeholder={t("topbar.searchPlaceholder")} disabled />
      </label>

      <button className="btn" disabled title={t("views.comingInPhase", { phase: 2 })}>
        <Globe size={16} aria-hidden />
        {t("topbar.searchOnline")}
      </button>
      <button className="btn" disabled title={t("views.comingInPhase", { phase: 1 })}>
        <PenLine size={16} aria-hidden />
        {t("topbar.rename")}
      </button>

      <div className="topbar-spacer drag-region" data-tauri-drag-region />

      <span className="status-pill" role="status">
        <span className={`status-dot ${online ? "online" : "offline"}`} aria-hidden />
        {online ? t("topbar.online") : t("topbar.offline")}
      </span>

      <button className="icon-btn" disabled aria-label={t("topbar.tasks")} title={t("topbar.tasks")}>
        <ListTodo size={20} aria-hidden />
      </button>
      <button className="icon-btn" onClick={onToggleTheme} aria-label={themeLabel} title={themeLabel}>
        {resolvedTheme === "dark" ? <Sun size={20} aria-hidden /> : <Moon size={20} aria-hidden />}
      </button>
      <button
        className="icon-btn"
        onClick={onToggleDetail}
        aria-pressed={detailOpen}
        aria-label={t("topbar.toggleDetail")}
        title={`${t("topbar.toggleDetail")} (Ctrl+I)`}
      >
        <PanelRight size={20} aria-hidden />
      </button>
    </header>
  );
}
