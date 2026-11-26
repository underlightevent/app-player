import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";
import {
  getSettings,
  isCoreError,
  logMessage,
  setSetting,
  startupStatus,
  type StartupStatus,
} from "./core/api";
import { applyTheme, isThemeSetting, resolveTheme, systemPrefersDark, type ThemeSetting } from "./core/theme";
import { LANGUAGES, setLanguage, type Language } from "./i18n";
import { Sidebar, type ViewId } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { MainView } from "./components/MainView";
import { DetailPanel } from "./components/DetailPanel";
import { PlayerBar } from "./components/PlayerBar";
import { Toast, useToast } from "./components/Toast";

function isLanguage(v: unknown): v is Language {
  return LANGUAGES.includes(v as Language);
}

export default function App() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<StartupStatus | null>(null);
  const [theme, setThemeState] = useState<ThemeSetting>("light");
  const [systemDark, setSystemDark] = useState(systemPrefersDark());
  const [language, setLanguageState] = useState<Language>("fr");
  const [view, setView] = useState<ViewId>("library");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [detailOpen, setDetailOpen] = useState(true);
  const toast = useToast();

  const resolved = resolveTheme(theme, systemDark);

  // Chargement initial : état du cœur et réglages enregistrés.
  useEffect(() => {
    startupStatus().then(setStatus).catch((e) => logMessage("error", `startup_status : ${String(e)}`));
    getSettings()
      .then((s) => {
        if (isThemeSetting(s.theme)) setThemeState(s.theme);
        if (isLanguage(s.language)) setLanguageState(s.language);
      })
      .catch((e) => logMessage("warn", `get_settings : ${isCoreError(e) ? e.detail : String(e)}`));
  }, []);

  // Suivre le thème du système en direct.
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    applyTheme(resolved);
  }, [resolved]);

  useEffect(() => {
    setLanguage(language);
  }, [language]);

  const save = useCallback(
    (key: string, value: string) => {
      setSetting(key, value).catch((e) => {
        logMessage("error", `set_setting ${key} : ${isCoreError(e) ? e.detail : String(e)}`);
        toast.show(t("errors.settingNotSaved"));
      });
    },
    [t, toast],
  );

  const changeTheme = useCallback(
    (next: ThemeSetting) => {
      setThemeState(next);
      try {
        localStorage.setItem("theme", next);
      } catch {
        /* simple confort : pas grave si indisponible */
      }
      save("theme", next);
    },
    [save],
  );

  const changeLanguage = useCallback(
    (next: Language) => {
      setLanguageState(next);
      save("language", next);
    },
    [save],
  );

  // Ctrl+I (Cmd+I sur macOS) : afficher ou masquer le panneau de détail.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key.toLowerCase() === "i") {
        e.preventDefault();
        setDetailOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const platformClass = status?.platform === "macos" ? "platform-macos" : "platform-other";
  const dbError = status && !status.database_ok ? status.database_error : null;

  return (
    <div className={`app ${platformClass} ${sidebarCollapsed ? "sidebar-collapsed" : ""}`}>
      <Sidebar
        current={view}
        onSelect={setView}
        collapsed={sidebarCollapsed}
        onToggleCollapsed={() => setSidebarCollapsed((c) => !c)}
      />
      <TopBar
        resolvedTheme={resolved}
        onToggleTheme={() => changeTheme(resolved === "dark" ? "light" : "dark")}
        detailOpen={detailOpen}
        onToggleDetail={() => setDetailOpen((o) => !o)}
      />
      <div className="workspace">
        <main className="main" id="main">
          {dbError && (
            <div className="banner-error" role="alert">
              <AlertTriangle size={20} aria-hidden />
              <div>
                <strong>{t("errors.title")}</strong>
                <div>{t(`errors.${dbError.code}`, { defaultValue: t("errors.internal") })}</div>
              </div>
            </div>
          )}
          <MainView
            view={view}
            theme={theme}
            onThemeChange={changeTheme}
            language={language}
            onLanguageChange={changeLanguage}
            version={status?.version ?? ""}
            onToast={toast.show}
          />
        </main>
        {detailOpen && <DetailPanel />}
      </div>
      <PlayerBar />
      <Toast message={toast.message} />
    </div>
  );
}
