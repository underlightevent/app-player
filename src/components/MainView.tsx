import { useTranslation } from "react-i18next";
import {
  CopyX,
  Folder,
  FolderPlus,
  HeartPulse,
  Inbox,
  ListChecks,
  ListMusic,
  ShieldAlert,
  type LucideIcon,
} from "lucide-react";
import type { ViewId } from "./Sidebar";
import { SettingsView } from "./SettingsView";
import type { ThemeSetting } from "../core/theme";
import type { Language } from "../i18n";

const EMPTY: Record<Exclude<ViewId, "library" | "settings">, { icon: LucideIcon; phase: number }> = {
  inbox: { icon: Inbox, phase: 2 },
  review: { icon: ListChecks, phase: 2 },
  duplicates: { icon: CopyX, phase: 3 },
  health: { icon: HeartPulse, phase: 3 },
  quarantine: { icon: ShieldAlert, phase: 3 },
  playlists: { icon: ListMusic, phase: 4 },
  folders: { icon: Folder, phase: 1 },
};

interface Props {
  view: ViewId;
  theme: ThemeSetting;
  onThemeChange: (t: ThemeSetting) => void;
  language: Language;
  onLanguageChange: (l: Language) => void;
  version: string;
  onToast: (msg: string) => void;
}

export function MainView(props: Props) {
  const { t } = useTranslation();
  const { view } = props;

  if (view === "settings") {
    return <SettingsView {...props} />;
  }

  if (view === "library") {
    return (
      <div className="empty">
        <FolderPlus size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
        <h1>{t("views.library.title")}</h1>
        <p>{t("views.library.text")}</p>
        <button className="btn btn-primary" disabled>
          <FolderPlus size={16} aria-hidden />
          {t("views.library.action")}
        </button>
        <span className="phase-note">{t("views.comingInPhase", { phase: 1 })}</span>
      </div>
    );
  }

  const { icon: Icon, phase } = EMPTY[view];
  return (
    <div className="empty">
      <Icon size={40} strokeWidth={1.5} className="empty-icon" aria-hidden />
      <h1>{t(`views.${view}.title`)}</h1>
      <p>{t(`views.${view}.text`)}</p>
      <span className="phase-note">{t("views.comingInPhase", { phase })}</span>
    </div>
  );
}
