import { useTranslation } from "react-i18next";
import {
  AudioLines,
  CopyX,
  Folder,
  HeartPulse,
  Inbox,
  Library,
  ListChecks,
  ListMusic,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  ShieldAlert,
  type LucideIcon,
} from "lucide-react";

export type ViewId =
  | "library"
  | "inbox"
  | "review"
  | "duplicates"
  | "health"
  | "quarantine"
  | "playlists"
  | "folders"
  | "settings";

interface NavEntry {
  id: ViewId;
  icon: LucideIcon;
}

const COLLECTION: NavEntry[] = [
  { id: "library", icon: Library },
  { id: "inbox", icon: Inbox },
  { id: "review", icon: ListChecks },
  { id: "duplicates", icon: CopyX },
];

const TOOLS: NavEntry[] = [
  { id: "health", icon: HeartPulse },
  { id: "quarantine", icon: ShieldAlert },
  { id: "playlists", icon: ListMusic },
  { id: "folders", icon: Folder },
];

interface Props {
  current: ViewId;
  onSelect: (v: ViewId) => void;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  /** Arborescence des dossiers, affichée sous « Dossiers ». */
  tree?: React.ReactNode;
  /** Nombre d'éléments à traiter par écran (Inbox, À valider, Doublons). */
  badges?: Partial<Record<ViewId, number>>;
}

export function Sidebar({ current, onSelect, collapsed, onToggleCollapsed, tree, badges }: Props) {
  const { t } = useTranslation();

  const item = ({ id, icon: Icon }: NavEntry) => (
    <button
      key={id}
      className="nav-item"
      aria-current={current === id ? "page" : undefined}
      onClick={() => onSelect(id)}
      title={collapsed ? t(`nav.${id}`) : undefined}
    >
      <Icon size={20} strokeWidth={1.75} aria-hidden />
      <span className="nav-label">{t(`nav.${id}`)}</span>
      {!!badges?.[id] && (
        <span className="nav-badge tabular" aria-label={t("nav.toHandle", { count: badges[id] })}>
          {badges[id]! > 999 ? "999+" : badges[id]}
        </span>
      )}
    </button>
  );

  return (
    <nav className="sidebar" aria-label={t("app.name")}>
      <div className="sidebar-header" data-tauri-drag-region>
        <span className="app-name" data-tauri-drag-region>
          <AudioLines size={16} aria-hidden style={{ verticalAlign: "-3px", marginRight: 6 }} />
          {t("app.name")}
        </span>
        <button
          className="icon-btn"
          onClick={onToggleCollapsed}
          aria-label={collapsed ? t("nav.expand") : t("nav.collapse")}
          title={collapsed ? t("nav.expand") : t("nav.collapse")}
        >
          {collapsed ? <PanelLeftOpen size={20} aria-hidden /> : <PanelLeftClose size={20} aria-hidden />}
        </button>
      </div>

      <div className="sidebar-section">
        <div className="sidebar-section-title">{t("nav.sectionCollection")}</div>
        {COLLECTION.map(item)}
      </div>
      <div className="sidebar-section">
        <div className="sidebar-section-title">{t("nav.sectionTools")}</div>
        {TOOLS.map(item)}
        {!collapsed && tree}
      </div>

      <div className="sidebar-footer">{item({ id: "settings", icon: Settings })}</div>
    </nav>
  );
}
