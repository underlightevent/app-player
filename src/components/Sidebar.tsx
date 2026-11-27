import { useTranslation } from "react-i18next";
import { useDropTarget } from "../core/dnd";
import {
  AudioLines,
  BarChart3,
  CopyX,
  Folder,
  HeartPulse,
  Inbox,
  Library,
  ListChecks,
  ListMusic,
  ListOrdered,
  PanelLeftClose,
  Radar,
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
  | "sets"
  | "stats"
  | "watch"
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
  { id: "sets", icon: ListOrdered },
  { id: "watch", icon: Radar },
  { id: "stats", icon: BarChart3 },
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
  /** Morceaux glissés depuis le tableau sur « Playlists » ou « Sets » (phase 6). */
  onDropTracks?: (ids: number[]) => void;
}

/** Entrée de la barre latérale ; « Playlists » et « Sets » acceptent des morceaux glissés
 * depuis le tableau (glisser-déposer interne, voir core/dnd). */
function NavItem({ entry, current, collapsed, badge, onSelect, onDropTracks }: {
  entry: NavEntry;
  current: ViewId;
  collapsed: boolean;
  badge?: number;
  onSelect: (v: ViewId) => void;
  onDropTracks?: (ids: number[]) => void;
}) {
  const { t } = useTranslation();
  const { id, icon: Icon } = entry;
  const accepts = (id === "playlists" || id === "sets") && !!onDropTracks;
  const ref = useDropTarget<HTMLButtonElement>("tracks", (p) => p.kind === "tracks" && onDropTracks?.(p.ids), accepts);
  return (
    <button
      ref={ref}
      className="nav-item"
      aria-current={current === id ? "page" : undefined}
      onClick={() => onSelect(id)}
      title={collapsed ? t(`nav.${id}`) : accepts ? t("nav.dropHint") : undefined}
    >
      <Icon size={20} strokeWidth={1.75} aria-hidden />
      <span className="nav-label">{t(`nav.${id}`)}</span>
      {!!badge && (
        <span className="nav-badge tabular" aria-label={t("nav.toHandle", { count: badge })}>
          {badge > 999 ? "999+" : badge}
        </span>
      )}
    </button>
  );
}

export function Sidebar({ current, onSelect, collapsed, onToggleCollapsed, tree, badges, onDropTracks }: Props) {
  const { t } = useTranslation();

  const item = (entry: NavEntry) => (
    <NavItem key={entry.id} entry={entry} current={current} collapsed={collapsed} badge={badges?.[entry.id]} onSelect={onSelect} onDropTracks={onDropTracks} />
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
