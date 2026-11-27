import { useTranslation } from "react-i18next";
import { BarChart3, CopyX, HeartPulse, Inbox, ListChecks, ListMusic, ListOrdered, Radar, ShieldAlert, type LucideIcon } from "lucide-react";
import type { ViewId } from "./Sidebar";
import { SettingsView, type SettingsProps } from "./SettingsView";

const EMPTY: Record<Exclude<ViewId, "library" | "settings" | "folders">, { icon: LucideIcon; phase: number }> = {
  inbox: { icon: Inbox, phase: 2 },
  review: { icon: ListChecks, phase: 2 },
  duplicates: { icon: CopyX, phase: 3 },
  health: { icon: HeartPulse, phase: 3 },
  quarantine: { icon: ShieldAlert, phase: 3 },
  playlists: { icon: ListMusic, phase: 4 },
  watch: { icon: Radar, phase: 5 },
  sets: { icon: ListOrdered, phase: 6 },
  stats: { icon: BarChart3, phase: 6 },
};

/** Écrans pas encore disponibles, et Réglages. */
export function OtherView({ view, settings }: { view: Exclude<ViewId, "library" | "folders">; settings: SettingsProps }) {
  const { t } = useTranslation();
  if (view === "settings") return <SettingsView {...settings} />;
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
