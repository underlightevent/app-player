import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { emptyQuery, libraryAddRoot, libraryRows, onEvent, pickFolder, type LibraryQuery, type TrackRow } from "./core/library";
import {
  clipboardImage,
  coverPrepare,
  imageData,
  historyApply,
  isImagePath,
  libraryGenres,
  pickImage,
  tracksEdit,
  tracksEditEach,
  tracksSetLock,
  type Changes,
  type EditableField,
  type EditReport,
} from "./core/edit";
import { useLibrary } from "./core/useLibrary";
import { applyTheme, isThemeSetting, resolveTheme, systemPrefersDark, type ThemeSetting } from "./core/theme";
import { LANGUAGES, setLanguage, type Language } from "./i18n";
import { Sidebar, type ViewId } from "./components/Sidebar";
import { TopBar, type MoreAction } from "./components/TopBar";
import { RenameDialog } from "./components/RenameDialog";
import { FolderTree, type FolderTarget } from "./components/FolderTree";
import { MoveDialog, NameDialog } from "./components/FolderDialogs";
import { Onboarding } from "./components/Onboarding";
import { ReviewView, ManualSearchPanel } from "./components/ReviewView";
import { InboxView } from "./components/InboxView";
import { CropEditor } from "./components/CropEditor";
import { CommandPalette, type PaletteAction } from "./components/CommandPalette";
import { emptyStatus, onlineSearch, onlineStatus, type OnlineStatus } from "./core/online";
import { databaseExport, databaseImport, folderCreate, folderMove, reveal } from "./core/folders";
import { CleanDialog, ParseDialog } from "./components/TagsPreviewDialog";
import { DEFAULT_TEMPLATE, renamePreview, type CleanRules, type SavedTemplate } from "./core/rename";
import { OtherView } from "./components/MainView";
import { LibraryView } from "./components/LibraryView";
import { FoldersView } from "./components/FoldersView";
import { DetailPanel } from "./components/DetailPanel";
import { PlayerBar } from "./components/PlayerBar";
import { useTasks } from "./components/TasksPanel";
import { initialPlayerState, playerCommand, playerLoad, playerState, type PlayerState } from "./core/player";
import { Toast, useToast } from "./components/Toast";
import { DuplicatesView } from "./components/DuplicatesView";
import { QuarantineView } from "./components/QuarantineView";
import { HealthView } from "./components/HealthView";
import { duplicatesCount, quarantineTracks } from "./core/quality";
import { PlaylistsView } from "./components/PlaylistsView";
import { AddToPlaylistDialog, ConvertDialog } from "./components/Phase4Dialogs";
import { Modal } from "./components/Modal";
import { exportM3u, openReleasePage, pickSave, playlistRemove, rekordboxStatus, updateCheck, type Playlist } from "./core/rekordbox";

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
  // Aperçu navigateur : « ?view=duplicates » ouvre directement un écran (captures du guide).
  const [view, setView] = useState<ViewId>(() => (!inTauri && new URLSearchParams(location.search).get("view")) as ViewId || "library");
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
        setSettingsValues(s);
        if (s.onboarding_done !== "true") setOnboarding(true);
      })
      .catch((e) => logMessage("warn", `get_settings : ${isCoreError(e) ? e.detail : String(e)}`));
  }, []);
  const [settingsValues, setSettingsValues] = useState<Record<string, string>>({});
  const [onboarding, setOnboarding] = useState(false);

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
      setSettingsValues((s) => ({ ...s, [key]: value }));
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
  // Liste stable des morceaux sélectionnés (évite de relancer les aperçus à chaque affichage).
  const selectedIdList = useMemo(() => [...selection], [selection]);

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
  const [cropRequest, setCropRequest] = useState<{ src: string; ids: number[] } | null>(null);

  // ---------- Recherche en ligne ----------
  const [online, setOnline] = useState<OnlineStatus>(emptyStatus);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [manualSearch, setManualSearch] = useState(false);
  useEffect(() => {
    void onlineStatus().then(setOnline).catch(() => {});
    const a = onEvent<OnlineStatus>("online:status", setOnline);
    const b = onEvent<number>("online:caught-up", (n) => toast.show(t("online.caughtUp", { count: n })));
    // Bilan d'une série de recherches, avec un accès direct à la file « À valider ».
    const c = onEvent<{ auto: number; review: number; unidentified: number; error: number }>("online:batch-done", (s) =>
      toast.show(
        t("online.batchDone", { auto: s.auto, review: s.review, unidentified: s.unidentified + s.error }),
        s.review + s.unidentified + s.error > 0 ? { label: t("online.view"), run: () => setView("review") } : undefined,
      ),
    );
    return () => {
      a();
      b();
      c();
    };
  }, [toast, t]);

  const searchOnline = useCallback(async () => {
    const ids = [...selection];
    if (ids.length === 0) {
      toast.show(t("edit.selectFirst"));
      return;
    }
    await onlineSearch(ids);
    toast.show(navigator.onLine ? t("online.queued", { count: ids.length }) : t("online.queuedOffline", { count: ids.length }));
  }, [selection, toast, t]);

  const setCoverFrom = useCallback(
    async (source: { path?: string; data?: string }) => {
      const ids = selectedIds();
      if (ids.length === 0) {
        toast.show(t("edit.selectFirst"));
        return;
      }
      try {
        const prepared = await coverPrepare(source);
        if (prepared.notSquare) {
          // Image nettement rectangulaire : l'éditeur de recadrage s'ouvre.
          const src = source.path ? await imageData(source.path) : `data:image/*;base64,${source.data}`;
          setCropRequest({ src, ids });
          return;
        }
        await applyEdit(ids, { cover: prepared.cover });
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

  // ---------- Renommage, nettoyage, tags depuis le nom ----------
  const [dialog, setDialog] = useState<null | "rename" | "clean" | "parse">(null);
  /** Renommage demandé depuis « Santé de la bibliothèque » (morceaux hors sélection). */
  const [renameIds, setRenameIds] = useState<number[] | null>(null);
  const [playlistDialog, setPlaylistDialog] = useState<number[] | null>(null);
  const [convertIds, setConvertIds] = useState<number[] | null>(null);
  const [quarantineAsk, setQuarantineAsk] = useState<{ ids: number[]; inRekordbox: number } | null>(null);
  const [openPlaylist, setOpenPlaylist] = useState<Playlist | null>(null);
  // Au démarrage : export Rekordbox trop ancien, nouvelle version disponible (si activé).
  useEffect(() => {
    if (!inTauri) return;
    void rekordboxStatus()
      .then((s) => {
        if (s.stale) toast.show(t("rekordbox.staleToast"), { label: t("rekordbox.reimport"), run: () => setView("settings") });
      })
      .catch(() => {});
    void getSettings().then((s) => {
      if (s.update_check === "false") return;
      void updateCheck()
        .then((u) => {
          if (u.newer) toast.show(t("update.available", { version: u.latest }), { label: t("update.download"), run: () => void openReleasePage(u.url) });
        })
        .catch(() => {});
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [dupCount, setDupCount] = useState(0);
  useEffect(() => {
    const refresh = () => void duplicatesCount().then(setDupCount).catch(() => {});
    refresh();
    return onEvent("library:changed", refresh);
  }, []);
  const [renameTemplate, setRenameTemplate] = useState(DEFAULT_TEMPLATE);
  const [savedTemplates, setSavedTemplates] = useState<SavedTemplate[]>([]);
  const [parsePattern, setParsePattern] = useState("");
  const [cleanRules, setCleanRules] = useState<CleanRules | null>(null);
  const [copiedTags, setCopiedTags] = useState<Changes | null>(null);
  useEffect(() => {
    getSettings()
      .then((s) => {
        if (s.rename_template) setRenameTemplate(s.rename_template);
        try {
          setSavedTemplates(JSON.parse(s.rename_templates || "[]"));
        } catch {
          /* réglage abîmé : liste vide */
        }
        if (s.parse_pattern) setParsePattern(s.parse_pattern);
        try {
          if (s.clean_rules) setCleanRules(JSON.parse(s.clean_rules));
        } catch {
          /* idem */
        }
      })
      .catch(() => {});
  }, []);

  const reportDone = useCallback(
    (r: EditReport, okKey: string) => {
      setRevision((n) => n + 1);
      const undo = r.batchId != null ? { label: t("common.undo"), run: () => void runHistory(false, r.batchId!) } : undefined;
      if (r.failures.length) toast.show(describeFailures(r), r.ok > 0 ? undo : undefined, "error");
      else toast.show(t(okKey, { count: r.ok }), undo);
    },
    [t, toast, runHistory, describeFailures],
  );

  const swapArtistTitle = useCallback(async () => {
    const items: [number, Changes][] = selectedRows.map((r) => [
      r.id,
      { title: r.artists.join(", ") || null, artists: r.title ? [r.title] : [] },
    ]);
    if (items.length) reportDone(await tracksEditEach(items, "history.swap"), "edit.saved");
  }, [selectedRows, reportDone]);

  const copyTags = useCallback(() => {
    const r = selectedRows[0];
    if (!r) return;
    setCopiedTags({ title: r.title, version: r.version, artists: r.artists, album: r.album, year: r.year, genre: r.genre, bpm: r.bpm, key: r.key, rating: r.rating });
    toast.show(t("edit.tagsCopied"));
  }, [selectedRows, toast, t]);

  const moreActions: MoreAction[] = [
    { id: "clean", label: t("actions.clean"), disabled: selection.size === 0, run: () => setDialog("clean") },
    { id: "parse", label: t("actions.parse"), disabled: selection.size === 0, run: () => setDialog("parse") },
    { id: "swap", label: t("actions.swap"), disabled: selection.size === 0, run: () => void swapArtistTitle() },
    { id: "copy", label: t("actions.copyTags"), disabled: selection.size !== 1, run: copyTags },
    { id: "paste", label: t("actions.pasteTags"), disabled: selection.size === 0 || !copiedTags, run: () => copiedTags && void applyEdit([...selection], copiedTags) },
    { id: "reveal", label: t("actions.reveal"), disabled: selection.size !== 1, run: () => void revealSelected() },
    {
      id: "quarantine",
      label: t("actions.quarantine"),
      disabled: selection.size === 0,
      run: async () => {
        const ids = [...selection];
        // Alerte avant de déplacer un morceau présent dans Rekordbox.
        const inRekordbox = selectedRows.filter((r) => r.inRekordbox).length;
        if (inRekordbox > 0) return setQuarantineAsk({ ids, inRekordbox });
        const r = await quarantineTracks(ids);
        setSelection(new Set());
        reportDone(r, "quarantine.done");
      },
    },
    { id: "playlist-add", label: t("actions.addToPlaylist"), disabled: selection.size === 0, run: () => setPlaylistDialog([...selection]) },
    ...(openPlaylist?.kind === "static" && query.playlist === openPlaylist.id
      ? [
          {
            id: "playlist-remove",
            label: t("actions.removeFromPlaylist", { name: openPlaylist.name }),
            disabled: selection.size === 0,
            run: async () => {
              await playlistRemove(openPlaylist.id, [...selection]);
              setSelection(new Set());
            },
          },
        ]
      : []),
    {
      id: "export-m3u",
      label: t("actions.exportM3u"),
      disabled: selection.size === 0,
      run: async () => {
        const path = await pickSave(t("playlists.exportM3u"), "selection.m3u8", "m3u8", "M3U8");
        if (!path) return;
        const [n] = await exportM3u([...selection], path);
        toast.show(t("playlists.exported", { count: n }));
      },
    },
    {
      id: "convert",
      label: t("actions.convertAiff"),
      disabled: !selectedRows.some((r) => r.format === "wav"),
      run: () => setConvertIds(selectedRows.filter((r) => r.format === "wav").map((r) => r.id)),
    },
  ];

  // ---------- Dossiers ----------
  const [folderRevision, setFolderRevision] = useState(0);
  const [moveRequest, setMoveRequest] = useState<{ ids: number[]; target: FolderTarget } | null>(null);
  const [nameRequest, setNameRequest] = useState<{ title: string; initial: string; submit: (n: string) => Promise<void> } | null>(null);

  async function revealSelected() {
    const id = [...selection][0];
    if (id == null) return;
    await reveal({ id }).catch(() => toast.show(t("errors.offlineFile"), undefined, "error"));
  }

  const folderDone = useCallback(
    (r: EditReport, okKey: string) => {
      setFolderRevision((n) => n + 1);
      reportDone(r, okKey);
    },
    [reportDone],
  );

  const folderTree = (
    <FolderTree
      roots={lib.roots}
      current={query.folder ? { volumeId: query.folder[0], relPath: query.folder[1], name: "" } : null}
      revision={folderRevision + revision}
      onOpen={(f) => {
        setQuery((q) => ({ ...q, folder: [f.volumeId, f.relPath] }));
        setView("library");
      }}
      onDropTracks={(ids, f) => setMoveRequest({ ids, target: f })}
      onCreate={(parent) =>
        setNameRequest({
          title: t("folders.newSubIn", { name: parent.name }),
          initial: "",
          submit: async (n) => folderDone(await folderCreate(parent.volumeId, parent.relPath, n), "folders.created"),
        })
      }
      onRename={(f) =>
        setNameRequest({
          title: t("folders.renameTitle", { name: f.name }),
          initial: f.name,
          submit: async (n) => {
            const parent = f.relPath.includes("/") ? f.relPath.slice(0, f.relPath.lastIndexOf("/")) : "";
            folderDone(await folderMove(f.volumeId, f.relPath, parent ? `${parent}/${n}` : n), "folders.renamed");
          },
        })
      }
      onReveal={(f) => void reveal({ volumeId: f.volumeId, rel: f.relPath })}
    />
  );

  // Aperçu du futur nom de fichier dans le panneau de détail.
  const [futureName, setFutureName] = useState<string | null>(null);
  useEffect(() => {
    const r = selectedRows.length === 1 ? selectedRows[0] : null;
    if (!r || !r.online) {
      setFutureName(null);
      return;
    }
    let stale = false;
    renamePreview([r.id], renameTemplate, "suffix")
      .then((p) => !stale && setFutureName(p[0]?.newName ?? null))
      .catch(() => !stale && setFutureName(null));
    return () => {
      stale = true;
    };
  }, [selectedRows, renameTemplate]);

  // ---------- Lecteur ----------
  const [player, setPlayer] = useState<PlayerState>(initialPlayerState);
  const [playingRow, setPlayingRow] = useState<TrackRow | null>(null);
  useEffect(() => {
    void playerState().then(setPlayer);
    return onEvent<PlayerState>("player:state", setPlayer);
  }, []);
  useEffect(() => {
    if (player.trackId == null) {
      setPlayingRow(null);
      return;
    }
    let stale = false;
    void libraryRows([player.trackId]).then((r) => !stale && setPlayingRow(r[0] ?? null));
    return () => {
      stale = true;
    };
  }, [player.trackId, revision]);

  const playTrack = useCallback(
    async (id: number) => {
      try {
        await playerLoad(id, true);
      } catch (e) {
        logMessage("warn", `Lecture : ${JSON.stringify(e)}`);
        toast.show(t("player.errors.offline"), undefined, "error");
      }
    },
    [toast, t],
  );

  /** Morceau voisin dans l'ordre du tableau (le morceau en cours, sinon la sélection). */
  const neighbour = useCallback(
    (delta: number): number | null => {
      const ref = player.trackId ?? [...selection][0];
      const i = ref == null ? -1 : lib.ids.indexOf(ref);
      const j = i + delta;
      return j >= 0 && j < lib.ids.length ? lib.ids[j] : null;
    },
    [player.trackId, selection, lib.ids],
  );

  const togglePlay = useCallback(() => {
    if (player.trackId == null) {
      const first = [...selection][0];
      if (first != null) void playTrack(first);
      return;
    }
    playerCommand(player.playing ? "pause" : "play");
  }, [player.trackId, player.playing, selection, playTrack]);

  // Fin du morceau : on enchaîne sur le suivant du tableau.
  const endedRef = useRef(false);
  useEffect(() => {
    if (player.ended && !endedRef.current) {
      endedRef.current = true;
      const next = neighbour(1);
      if (next != null) void playTrack(next);
    } else if (!player.ended) {
      endedRef.current = false;
    }
  }, [player.ended, neighbour, playTrack]);

  // Raccourcis du lecteur.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable)) return;
      if (dialog) return;
      if (e.key === " " && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        togglePlay();
      } else if (e.shiftKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        e.preventDefault();
        const id = neighbour(e.key === "ArrowLeft" ? -1 : 1);
        if (id != null) void playTrack(id);
      } else if (!e.shiftKey && !e.ctrlKey && !e.metaKey && (e.key === "ArrowLeft" || e.key === "ArrowRight") && player.trackId != null) {
        e.preventDefault();
        const ms = player.positionMs + (e.key === "ArrowLeft" ? -10_000 : 10_000);
        playerCommand("seek", Math.min(Math.max(0, ms), player.durationMs));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [togglePlay, neighbour, playTrack, player.trackId, player.positionMs, player.durationMs, dialog]);

  // ---------- Palette de commandes ----------
  const go = (v: ViewId) => () => setView(v);
  const paletteActions: PaletteAction[] = [
    { id: "search-online", label: t("topbar.searchOnline"), hint: `${modKey}+L`, run: () => void searchOnline() },
    { id: "rename", label: t("topbar.rename"), hint: `${modKey}+R`, run: () => selection.size && setDialog("rename") },
    ...moreActions.filter((a) => !a.disabled).map((a) => ({ id: a.id, label: a.label, run: a.run })),
    { id: "add-folder", label: t("views.library.action"), run: () => void addFolder() },
    { id: "theme", label: resolved === "dark" ? t("topbar.themeToLight") : t("topbar.themeToDark"), run: () => changeTheme(resolved === "dark" ? "light" : "dark") },
    { id: "detail", label: t("topbar.toggleDetail"), hint: `${modKey}+I`, run: () => setDetailOpen((o) => !o) },
    { id: "undo", label: t("palette.undo"), hint: `${modKey}+Z`, run: () => void runHistory(false) },
    { id: "redo", label: t("palette.redo"), hint: `${modKey}+Maj+Z`, run: () => void runHistory(true) },
    ...(["library", "inbox", "review", "folders", "settings"] as ViewId[]).map((v) => ({ id: `go-${v}`, label: t("palette.goTo", { screen: t(`nav.${v}`) }), run: go(v) })),
  ];

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
      } else if (mod && k === "k") {
        e.preventDefault();
        setPaletteOpen(true);
      } else if (mod && k === "l") {
        e.preventDefault();
        void searchOnline();
      } else if (mod && k === "r" && !e.shiftKey) {
        e.preventDefault();
        if (selection.size > 0) setDialog("rename");
      } else if (mod && k === "r" && e.shiftKey) {
        e.preventDefault();
        void revealSelected();
      } else if (e.key === "F2" && selection.size > 0) {
        // F2 : modifier le titre du morceau sélectionné (panneau de détail).
        e.preventDefault();
        setDetailOpen(true);
        window.setTimeout(() => {
          const el = document.getElementById("f-title") as HTMLInputElement | null;
          el?.focus();
          el?.select();
        }, 50);
      } else if (!mod && !e.altKey && /^[0-5]$/.test(e.key) && selection.size > 0 && view === "library") {
        e.preventDefault();
        void applyEdit([...selection], { rating: Number(e.key) });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runHistory, pasteCover, applyEdit, selection, view]);

  const platformClass = isMac ? "platform-macos" : "platform-other";
  const dbError = status && !status.database_ok ? status.database_error : null;

  return (
    <div className={`app ${platformClass} ${sidebarCollapsed ? "sidebar-collapsed" : ""}`}>
      <Sidebar current={view} onSelect={setView} collapsed={sidebarCollapsed} onToggleCollapsed={() => setSidebarCollapsed((c) => !c)} tree={folderTree} badges={{ inbox: online.inbox, review: online.review, duplicates: dupCount }} />
      <TopBar
        onSearchOnline={() => void searchOnline()}
        onlineQueued={online.queued}
        onlineCoreOffline={online.offline}
        onRename={() => setDialog("rename")}
        renameDisabled={selection.size === 0}
        moreActions={moreActions}
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
              playlistName={query.playlist != null && openPlaylist?.id === query.playlist ? openPlaylist.name : null}
              query={query}
              onQueryChange={setQuery}
              ids={lib.ids}
              rows={lib.rows}
              staleRows={lib.staleRows}
              rowsVersion={lib.rowsVersion}
              needRows={lib.needRows}
              loading={lib.loading}
              roots={lib.roots}
              volumes={lib.volumes}
              selection={selection}
              onSelectionChange={(s) => setSelection(s)}
              onActivate={(id) => void playTrack(id)}
              onAddFolder={() => void addFolder()}
              density={density}
              keyNotation={keyNotation}
            />
          ) : view === "folders" ? (
            <FoldersView roots={lib.roots} onAddFolder={() => void addFolder()} onChanged={() => void lib.refresh()} onToast={toast.show} />
          ) : view === "review" ? (
            <ReviewView onDone={(r) => reportDone(r, "edit.saved")} />
          ) : view === "duplicates" ? (
            <DuplicatesView
              playingId={player.trackId}
              playing={player.playing}
              onPlay={(id) => void playTrack(id)}
              onToggle={togglePlay}
              onDone={reportDone}
              onToast={(text) => toast.show(text)}
            />
          ) : view === "playlists" ? (
            <PlaylistsView
              selection={selectedIdList}
              onToast={toast.show}
              onOpen={(p) => {
                setOpenPlaylist(p);
                setQuery({ ...emptyQuery, playlist: p.id });
                setView("library");
              }}
            />
          ) : view === "quarantine" ? (
            <QuarantineView onDone={reportDone} />
          ) : view === "health" ? (
            <HealthView
              onOpen={(key) => {
                setQuery({ ...emptyQuery, health: key });
                setView("library");
              }}
              onGo={setView}
              onRename={(ids) => {
                setRenameIds(ids);
                setDialog("rename");
              }}
              onDone={reportDone}
              onToast={toast.show}
            />
          ) : view === "inbox" ? (
            <InboxView
              roots={lib.roots}
              lastTarget={settingsValues.inbox_last_target ?? ""}
              onAddInbox={async () => {
                const p = await pickFolder(t("onboarding.inboxPick"));
                if (p) {
                  await libraryAddRoot(p, "inbox").catch(() => toast.show(t("library.notAFolder"), undefined, "error"));
                  void lib.refresh();
                }
              }}
              onDone={(r, key) => {
                reportDone(r, key);
                void getSettings().then(setSettingsValues);
              }}
            />
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
                values: settingsValues,
                onExport: async () => {
                  try {
                    if (await databaseExport(t("settings.export"))) toast.show(t("settings.exported"));
                  } catch {
                    toast.show(t("errors.internal"), undefined, "error");
                  }
                },
                onImport: async () => {
                  try {
                    if (await databaseImport(t("settings.import"))) toast.show(t("settings.imported"));
                  } catch (e) {
                    toast.show(isCoreError(e) ? t(`errors.${e.code}`, { defaultValue: e.detail }) : t("errors.internal"), undefined, "error");
                  }
                },
                version: status?.version ?? "",
                onToast: toast.show,
                selection: selectedIdList,
                onOpenRekordboxList: (kind) => {
                  setQuery({ ...emptyQuery, rekordbox: kind });
                  setView("library");
                },
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
            futureName={futureName}
            onManualSearch={() => setManualSearch(true)}
          />
        )}
      </div>
      {manualSearch && selectedRows.length === 1 && (
        <ManualSearchPanel
          trackId={selectedRows[0].id}
          current={{ ...selectedRows[0] }}
          hasCover={selectedRows[0].coverCount > 0}
          onClose={() => setManualSearch(false)}
          onDone={(r) => reportDone(r, "edit.saved")}
        />
      )}
      {cropRequest && (
        <CropEditor
          src={cropRequest.src}
          onCancel={() => setCropRequest(null)}
          onConfirm={async (edit) => {
            const req = cropRequest;
            setCropRequest(null);
            try {
              const p = await coverPrepare({ data: req.src.split(",")[1] }, edit);
              await applyEdit(req.ids, { cover: p.cover });
            } catch {
              toast.show(t("edit.coverUnreadable"), undefined, "error");
            }
          }}
        />
      )}
      {paletteOpen && (
        <CommandPalette
          actions={paletteActions}
          onClose={() => setPaletteOpen(false)}
          onTrack={(r) => {
            setView("library");
            setQuery((q) => ({ ...q, search: "", folder: null }));
            setSelection(new Set([r.id]));
          }}
        />
      )}
      {dialog === "rename" && (
        <RenameDialog
          ids={renameIds ?? selectedIdList}
          template={renameTemplate}
          saved={savedTemplates}
          onSaveTemplates={(tpl, list) => {
            setRenameTemplate(tpl);
            setSavedTemplates(list);
            save("rename_template", tpl);
            save("rename_templates", JSON.stringify(list));
          }}
          onClose={() => {
            setDialog(null);
            setRenameIds(null);
          }}
          onDone={(r) => reportDone(r, "rename.done")}
        />
      )}
      {dialog === "clean" && (
        <CleanDialog
          ids={selectedIdList}
          initial={cleanRules}
          onSaveRules={(r) => {
            setCleanRules(r);
            save("clean_rules", JSON.stringify(r));
          }}
          onClose={() => setDialog(null)}
          onDone={(r) => reportDone(r, "edit.saved")}
        />
      )}
      {dialog === "parse" && (
        <ParseDialog
          ids={selectedIdList}
          initial={parsePattern}
          onSavePattern={(p) => {
            setParsePattern(p);
            save("parse_pattern", p);
          }}
          onClose={() => setDialog(null)}
          onDone={(r) => reportDone(r, "edit.saved")}
        />
      )}
      <PlayerBar
        state={player}
        track={playingRow}
        onToggle={togglePlay}
        onPrev={() => {
          // Comme sur les lecteurs habituels : après 3 s, « précédent » revient au début.
          if (player.positionMs > 3000) playerCommand("seek", 0);
          else {
            const id = neighbour(-1);
            if (id != null) void playTrack(id);
          }
        }}
        onNext={() => {
          const id = neighbour(1);
          if (id != null) void playTrack(id);
        }}
        onSeek={(ms) => playerCommand("seek", ms)}
        onVolume={(v) => playerCommand("volume", v)}
      />
      {moreActions.length > 0 && moveRequest && (
        <MoveDialog ids={moveRequest.ids} target={moveRequest.target} onClose={() => setMoveRequest(null)} onDone={(r) => folderDone(r, "move.done")} />
      )}
      {playlistDialog && <AddToPlaylistDialog ids={playlistDialog} onClose={() => setPlaylistDialog(null)} onDone={(text) => toast.show(text)} />}
      {convertIds && <ConvertDialog ids={convertIds} onClose={() => setConvertIds(null)} onDone={(r) => reportDone(r, "convert.done")} />}
      {quarantineAsk && (
        <Modal
          title={t("rekordbox.quarantineTitle")}
          onClose={() => setQuarantineAsk(null)}
          footer={
            <>
              <button className="btn" onClick={() => setQuarantineAsk(null)}>{t("common.cancel")}</button>
              <button
                className="btn btn-primary"
                onClick={async () => {
                  const ids = quarantineAsk.ids;
                  setQuarantineAsk(null);
                  const r = await quarantineTracks(ids);
                  setSelection(new Set());
                  reportDone(r, "quarantine.done");
                }}
              >
                {t("actions.quarantine")}
              </button>
            </>
          }
        >
          <p className="rekordbox-alert" role="alert">{t("rekordbox.quarantineAlertSelection", { count: quarantineAsk.inRekordbox })}</p>
        </Modal>
      )}
      {nameRequest && <NameDialog title={nameRequest.title} initial={nameRequest.initial} onClose={() => setNameRequest(null)} onSubmit={nameRequest.submit} />}
      {onboarding && (
        <Onboarding
          theme={theme}
          onThemeChange={changeTheme}
          language={language}
          onLanguageChange={changeLanguage}
          onFinish={() => {
            setOnboarding(false);
            changeSetting("onboarding_done", "true");
            void lib.refresh();
          }}
        />
      )}
      <Toast toast={toast.toast} onHide={toast.hide} />
    </div>
  );
}
