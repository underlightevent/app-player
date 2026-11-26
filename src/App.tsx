import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle } from "lucide-react";
import {
  getSettings,
  inTauri,
  isCoreError,
  logMessage,
  setSetting,
  startupStatus,
  type StartupStatus,
} from "./core/api";
import { emptyQuery, libraryAddRoot, libraryRows, pickFolder, type LibraryQuery, type TrackRow } from "./core/library";
import { useLibrary } from "./core/useLibrary";
import { applyTheme, isThemeSetting, resolveTheme, systemPrefersDark, type ThemeSetting } from "./core/theme";
import { LANGUAGES, setLanguage, type Language } from "./i18n";
import { Sidebar, type ViewId } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { OtherView } from "./components/MainView";
import { LibraryView } from "./components/LibraryView";
import { FoldersView } from "./components/FoldersView";
import { DetailPanel } from "./components/DetailPanel";
import { PlayerBar } from "./components/PlayerBar";
import { useTasks } from "./components/TasksPanel";
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
  const [density, setDensity] = useState<"compact" | "comfortable">("compact");
  const [keyNotation, setKeyNotation] = useState<"camelot" | "musical">("camelot");
  const [view, setView] = useState<ViewId>("library");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [detailOpen, setDetailOpen] = useState(true);
  const [query, setQuery] = useState<LibraryQuery>(emptyQuery);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const toast = useToast();
  const lib = useLibrary(query);
  const tasks = useTasks();

  const resolved = resolveTheme(theme, systemDark);
  const isMac = status?.platform === "macos" || (!inTauri && navigator.userAgent.includes("Mac"));
  const modKey = isMac ? "Cmd" : "Ctrl";

  // Chargement initial : état du cœur et réglages enregistrés.
  useEffect(() => {
    startupStatus().then(setStatus).catch((e) => logMessage("error", `startup_status : ${String(e)}`));
    getSettings()
      .then((s) => {
        if (isThemeSetting(s.theme)) setThemeState(s.theme);
        if (isLanguage(s.language)) setLanguageState(s.language);
        if (s.table_density === "comfortable" || s.table_density === "compact") setDensity(s.table_density);
        if (s.key_notation === "camelot" || s.key_notation === "musical") setKeyNotation(s.key_notation);
      })
      .catch((e) => logMessage("warn", `get_settings : ${isCoreError(e) ? e.detail : String(e)}`));
  }, []);

  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => applyTheme(resolved), [resolved]);
  useEffect(() => setLanguage(language), [language]);

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
        /* simple confort */
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

  const changeSetting = useCallback(
    (key: string, value: string) => {
      if (key === "table_density") setDensity(value as "compact" | "comfortable");
      if (key === "key_notation") setKeyNotation(value as "camelot" | "musical");
      save(key, value);
    },
    [save],
  );

  const addFolder = useCallback(
    async (path?: string) => {
      const chosen = path ?? (await pickFolder(t("views.library.pickTitle")));
      if (!chosen) return;
      try {
        await libraryAddRoot(chosen);
        toast.show(t("library.folderAdded"));
        setView("library");
        void lib.refresh();
      } catch (e) {
        logMessage("warn", `Ajout de dossier : ${isCoreError(e) ? e.detail : String(e)}`);
        toast.show(isCoreError(e) && e.code === "io" ? t("library.notAFolder") : t("errors.internal"));
      }
    },
    [t, toast, lib],
  );

  // Glisser-déposer de dossiers depuis l'Explorateur ou le Finder.
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    let cancelled = false;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === "drop") for (const p of e.payload.paths) void addFolder(p);
        })
        .then((f) => {
          if (cancelled) f();
          else un = f;
        }),
    );
    return () => {
      cancelled = true;
      un?.();
    };
  }, [addFolder]);

  // Ctrl+I (Cmd+I) : afficher ou masquer le panneau de détail.
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

  // La sélection ne garde que les morceaux encore affichés.
  useEffect(() => {
    setSelection((sel) => {
      if (sel.size === 0) return sel;
      const visible = new Set(lib.ids);
      const next = new Set([...sel].filter((id) => visible.has(id)));
      return next.size === sel.size ? sel : next;
    });
  }, [lib.ids]);

  // Le panneau de détail charge lui-même les morceaux sélectionnés (ils peuvent être hors de l'écran).
  const [selectedRows, setSelectedRows] = useState<TrackRow[]>([]);
  useEffect(() => {
    const wanted = [...selection].slice(0, 500);
    if (wanted.length === 0) {
      setSelectedRows([]);
      return;
    }
    let stale = false;
    libraryRows(wanted)
      .then((r) => {
        if (!stale) setSelectedRows(r);
      })
      .catch((e) => logMessage("warn", `Sélection : ${JSON.stringify(e)}`));
    return () => {
      stale = true;
    };
  }, [selection, lib.ids]);

  const platformClass = isMac ? "platform-macos" : "platform-other";
  const dbError = status && !status.database_ok ? status.database_error : null;

  return (
    <div className={`app ${platformClass} ${sidebarCollapsed ? "sidebar-collapsed" : ""}`}>
      <Sidebar current={view} onSelect={setView} collapsed={sidebarCollapsed} onToggleCollapsed={() => setSidebarCollapsed((c) => !c)} />
      <TopBar
        search={query.search}
        onSearchChange={(search) => {
          setQuery((q) => ({ ...q, search }));
          if (view !== "library") setView("library");
        }}
        resolvedTheme={resolved}
        onToggleTheme={() => changeTheme(resolved === "dark" ? "light" : "dark")}
        detailOpen={detailOpen}
        onToggleDetail={() => setDetailOpen((o) => !o)}
        tasks={tasks.list}
        runningTasks={tasks.running.length}
        modKey={modKey}
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
          {view === "library" ? (
            <LibraryView
              query={query}
              onQueryChange={setQuery}
              ids={lib.ids}
              rows={lib.rows}
              staleRows={lib.staleRows}
              needRows={lib.needRows}
              loading={lib.loading}
              roots={lib.roots}
              volumes={lib.volumes}
              selection={selection}
              onSelectionChange={(s) => setSelection(s)}
              onAddFolder={() => void addFolder()}
              density={density}
              keyNotation={keyNotation}
            />
          ) : view === "folders" ? (
            <FoldersView roots={lib.roots} onAddFolder={() => void addFolder()} onChanged={() => void lib.refresh()} onToast={toast.show} />
          ) : (
            <OtherView
              view={view}
              settings={{
                theme,
                onThemeChange: changeTheme,
                language,
                onLanguageChange: changeLanguage,
                density,
                keyNotation,
                onSettingChange: changeSetting,
                version: status?.version ?? "",
                onToast: toast.show,
              }}
            />
          )}
        </main>
        {detailOpen && <DetailPanel selected={selectedRows} selectionSize={selection.size} keyNotation={keyNotation} />}
      </div>
      <PlayerBar />
      <Toast message={toast.message} />
    </div>
  );
}
