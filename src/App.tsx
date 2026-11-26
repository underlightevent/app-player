import { useCallback, useEffect, useRef, useState } from "react";
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
import {
  clipboardImage,
  coverPrepare,
  historyApply,
  isImagePath,
  libraryGenres,
  pickImage,
  tracksEdit,
  tracksSetLock,
  type Changes,
  type EditableField,
  type EditReport,
} from "./core/edit";
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

  // Glisser-déposer depuis l'Explorateur ou le Finder : images → pochette de la sélection,
  // dossiers → ajoutés à la bibliothèque.
  const onDropRef = useRef<(paths: string[]) => void>(() => {});
  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    let cancelled = false;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type === "drop") onDropRef.current(e.payload.paths);
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
  }, []);

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

  // ---------- Modification des tags, annulation ----------
  const [revision, setRevision] = useState(0);
  const [genres, setGenres] = useState<string[]>([]);
  useEffect(() => {
    void libraryGenres().then(setGenres).catch(() => {});
  }, [lib.ids]);

  const describeFailures = useCallback(
    (r: EditReport) => {
      const f = r.failures[0];
      const reason = t(`writeErrors.${f.code}`, { defaultValue: f.message });
      return r.failures.length === 1
        ? t("edit.failedOne", { file: f.fileName, reason })
        : t("edit.failedMany", { count: r.failures.length, reason });
    },
    [t],
  );

  const runHistory = useCallback(
    async (redo: boolean, batch?: number) => {
      try {
        const r = await historyApply(redo, batch);
        if (!r.label) {
          toast.show(redo ? t("edit.nothingToRedo") : t("edit.nothingToUndo"));
          return;
        }
        setRevision((n) => n + 1);
        if (r.report.failures.length) toast.show(describeFailures(r.report), undefined, "error");
        else toast.show(redo ? t("edit.redone", { what: t(r.label) }) : t("edit.undone", { what: t(r.label) }));
      } catch (e) {
        logMessage("error", `Historique : ${JSON.stringify(e)}`);
        toast.show(t("errors.internal"), undefined, "error");
      }
    },
    [t, toast, describeFailures],
  );

  const applyEdit = useCallback(
    async (ids: number[], changes: Changes) => {
      if (ids.length === 0) return;
      try {
        const r = await tracksEdit(ids, changes);
        setRevision((n) => n + 1);
        const undo = r.batchId != null ? { label: t("common.undo"), run: () => void runHistory(false, r.batchId!) } : undefined;
        if (r.failures.length) toast.show(describeFailures(r), r.ok > 0 ? undo : undefined, "error");
        else toast.show(t("edit.saved", { count: r.ok }), undo);
      } catch (e) {
        logMessage("error", `Modification : ${JSON.stringify(e)}`);
        toast.show(t("errors.internal"), undefined, "error");
      }
    },
    [t, toast, runHistory, describeFailures],
  );

  const selectedIds = useCallback(() => [...selection], [selection]);

  const setCoverFrom = useCallback(
    async (source: { path?: string; data?: string }) => {
      const ids = selectedIds();
      if (ids.length === 0) {
        toast.show(t("edit.selectFirst"));
        return;
      }
      try {
        const prepared = await coverPrepare(source);
        await applyEdit(ids, { cover: prepared.cover });
        if (prepared.notSquare) toast.show(t("edit.coverNotSquare"));
      } catch (e) {
        logMessage("warn", `Pochette : ${JSON.stringify(e)}`);
        toast.show(t("edit.coverUnreadable"), undefined, "error");
      }
    },
    [selectedIds, applyEdit, toast, t],
  );

  const pickCover = useCallback(async () => {
    const path = await pickImage(t("detail.coverPick"));
    if (path) await setCoverFrom({ path });
  }, [setCoverFrom, t]);

  const pasteCover = useCallback(async () => {
    const data = await clipboardImage();
    if (data) await setCoverFrom({ data });
    else toast.show(t("edit.noImageInClipboard"));
  }, [setCoverFrom, toast, t]);

  onDropRef.current = (paths: string[]) => {
    const images = paths.filter(isImagePath);
    if (images.length > 0) void setCoverFrom({ path: images[0] });
    for (const p of paths.filter((x) => !isImagePath(x))) void addFolder(p);
  };

  const lockFields = useCallback(
    async (field: EditableField, locked: boolean) => {
      await tracksSetLock([...selection], field, locked);
      setRevision((n) => n + 1);
      toast.show(locked ? t("edit.locked", { field: t(`fields.${field}`) }) : t("edit.unlocked", { field: t(`fields.${field}`) }));
    },
    [selection, toast, t],
  );

  // Raccourcis : annuler / rétablir, note 0 à 5, coller une pochette.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing = !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT" || target.isContentEditable);
      const mod = e.ctrlKey || e.metaKey;
      if (typing) return;
      const k = e.key.toLowerCase();
      if (mod && k === "z") {
        e.preventDefault();
        void runHistory(e.shiftKey);
      } else if (mod && k === "y") {
        e.preventDefault();
        void runHistory(true);
      } else if (mod && k === "v") {
        e.preventDefault();
        void pasteCover();
      } else if (!mod && !e.altKey && /^[0-5]$/.test(e.key) && selection.size > 0 && view === "library") {
        e.preventDefault();
        void applyEdit([...selection], { rating: Number(e.key) });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runHistory, pasteCover, applyEdit, selection, view]);

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
  }, [selection, lib.ids, revision]);

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
        {detailOpen && (
          <DetailPanel
            selected={selectedRows}
            selectionSize={selection.size}
            keyNotation={keyNotation}
            genres={genres}
            onEdit={(c) => void applyEdit([...selection], c)}
            onLock={(f, l) => void lockFields(f, l)}
            onPickCover={() => void pickCover()}
            onPasteCover={() => void pasteCover()}
            revision={revision}
          />
        )}
      </div>
      <PlayerBar />
      <Toast toast={toast.toast} onHide={toast.hide} />
    </div>
  );
}
