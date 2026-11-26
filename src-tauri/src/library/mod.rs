//! Bibliothèque : dossiers racines, disques, indexation en tâche de fond,
//! surveillance des changements faits en dehors de l'application.

pub mod edit;
pub mod folders;
pub mod index;
pub mod query;
pub mod volumes;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::db;
use crate::error::AppError;
use crate::tags::is_audio_path;
use crate::tasks::Tasks;
use index::RootRow;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeStatus {
    pub id: i64,
    pub label: String,
    pub online: bool,
    pub mount: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    pub id: i64,
    pub volume_id: i64,
    pub volume_label: String,
    pub rel_path: String,
    pub path: Option<String>,
    pub online: bool,
    pub kind: String,
    pub track_count: i64,
}

pub struct Library {
    db_path: PathBuf,
    pub(crate) mounts: RwLock<HashMap<i64, PathBuf>>,
    pub tasks: Arc<Tasks>,
    app: Option<AppHandle>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    scan_lock: Mutex<()>,
    hashing: AtomicBool,
    last_changed_emit: AtomicU64,
    started: Instant,
}

impl Library {
    pub fn new(db_path: PathBuf, tasks: Arc<Tasks>, app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Library {
            db_path,
            mounts: RwLock::new(HashMap::new()),
            tasks,
            app,
            watcher: Mutex::new(None),
            scan_lock: Mutex::new(()),
            hashing: AtomicBool::new(false),
            last_changed_emit: AtomicU64::new(0),
            started: Instant::now(),
        })
    }

    fn conn(&self) -> Result<Connection, AppError> {
        db::open_worker(&self.db_path)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn mounts(&self) -> HashMap<i64, PathBuf> {
        self.mounts.read().map(|m| m.clone()).unwrap_or_default()
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, payload);
        }
    }

    /// Prévient l'interface que la liste des morceaux a changé (au plus ~3 fois par seconde).
    pub fn notify_changed(&self, force: bool) {
        let now = self.started.elapsed().as_millis() as u64;
        let last = self.last_changed_emit.load(Ordering::Relaxed);
        if force || now.saturating_sub(last) > 350 {
            self.last_changed_emit.store(now, Ordering::Relaxed);
            self.emit("library:changed", ());
        }
    }

    /// Met à jour la table des disques branchés. Renvoie les volumes qui viennent d'apparaître.
    pub fn refresh_mounts(&self) -> Result<Vec<i64>, AppError> {
        let conn = self.conn()?;
        let mounted = volumes::mounted();
        let mut stmt = conn.prepare("SELECT id, uid FROM volumes")?;
        let known: Vec<(i64, String)> =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        let mut next: HashMap<i64, PathBuf> = HashMap::new();
        for (id, uid) in &known {
            if let Some(m) = mounted.iter().find(|m| &m.uid == uid) {
                next.insert(*id, m.mount.clone());
                conn.execute(
                    "UPDATE volumes SET label = ?2, last_mount = ?3 WHERE id = ?1",
                    params![id, m.label, m.mount.to_string_lossy()],
                )?;
            }
        }
        let previous = self.mounts();
        let appeared: Vec<i64> = next.keys().filter(|k| previous.get(k) != next.get(k)).copied().collect();
        let changed = previous != next;
        if let Ok(mut m) = self.mounts.write() {
            *m = next;
        }
        if changed {
            log::info!("Disques : {} en ligne", self.mounts().len());
            if let Ok(v) = self.volumes() {
                self.emit("library:volumes", v);
            }
            self.notify_changed(true);
        }
        Ok(appeared)
    }

    pub fn volumes(&self) -> Result<Vec<VolumeStatus>, AppError> {
        let conn = self.conn()?;
        let mounts = self.mounts();
        let mut stmt = conn.prepare("SELECT id, label FROM volumes ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, label)| VolumeStatus {
                id,
                label,
                online: mounts.contains_key(&id),
                mount: mounts.get(&id).map(|m| m.to_string_lossy().into_owned()),
            })
            .collect())
    }

    pub fn roots(&self) -> Result<Vec<RootInfo>, AppError> {
        let conn = self.conn()?;
        let mounts = self.mounts();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.volume_id, v.label, r.rel_path, r.kind,
                    (SELECT count(*) FROM tracks t WHERE t.root_id = r.id AND t.status != 'missing')
             FROM roots r JOIN volumes v ON v.id = r.volume_id ORDER BY r.id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let volume_id: i64 = r.get(1)?;
                let rel: String = r.get(3)?;
                let mount = mounts.get(&volume_id);
                Ok(RootInfo {
                    id: r.get(0)?,
                    volume_id,
                    volume_label: r.get(2)?,
                    path: mount.map(|m| volumes::join_rel(m, &rel).to_string_lossy().into_owned()),
                    online: mount.is_some(),
                    rel_path: rel,
                    kind: r.get(4)?,
                    track_count: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Ajoute un dossier racine et lance son indexation.
    pub fn add_root(self: &Arc<Self>, path: &Path, kind: &str) -> Result<i64, AppError> {
        if !path.is_dir() {
            return Err(AppError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "ce n'est pas un dossier")));
        }
        let (vol, rel) = volumes::locate(path)?;
        let conn = self.conn()?;
        let vol_id = index::upsert_volume(&conn, &vol)?;
        // Un dossier déjà couvert par une racine existante n'est pas ajouté deux fois.
        for r in index::roots(&conn)? {
            if r.volume_id == vol_id && (rel == r.rel_path || rel.starts_with(&format!("{}/", r.rel_path)) || r.rel_path.is_empty()) {
                return Ok(r.id);
            }
        }
        let id = index::add_root(&conn, vol_id, &rel, kind)?;
        // Racines déjà présentes à l'intérieur du nouveau dossier : fusionnées dans celui-ci.
        conn.execute(
            "UPDATE tracks SET root_id = ?1 WHERE volume_id = ?2 AND root_id IN
               (SELECT id FROM roots WHERE volume_id = ?2 AND id != ?1 AND substr(rel_path, 1, length(?3)) = ?3)",
            params![id, vol_id, if rel.is_empty() { String::new() } else { format!("{rel}/") }],
        )?;
        conn.execute(
            "DELETE FROM roots WHERE volume_id = ?2 AND id != ?1 AND substr(rel_path, 1, length(?3)) = ?3",
            params![id, vol_id, if rel.is_empty() { String::new() } else { format!("{rel}/") }],
        )?;
        log::info!("Dossier ajouté : {} (volume {})", path.display(), vol.label);
        self.refresh_mounts()?;
        self.spawn_scan(Some(vec![id]));
        self.restart_watcher();
        Ok(id)
    }

    /// Retire un dossier de la bibliothèque (les fichiers ne sont pas touchés).
    pub fn remove_root(self: &Arc<Self>, id: i64) -> Result<(), AppError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM tracks WHERE root_id = ?1", [id])?;
        tx.execute("DELETE FROM roots WHERE id = ?1", [id])?;
        tx.commit()?;
        self.restart_watcher();
        self.notify_changed(true);
        Ok(())
    }

    /// Indexation en tâche de fond (toutes les racines en ligne si `root_ids` vaut None).
    pub fn spawn_scan(self: &Arc<Self>, root_ids: Option<Vec<i64>>) {
        let me = self.clone();
        std::thread::spawn(move || {
            let _guard = me.scan_lock.lock();
            let task = me.tasks.start("tasks.scan", None);
            let result = (|| -> Result<(), AppError> {
                let mut conn = me.conn()?;
                let exceptions = index::artist_exceptions(&conn)?;
                let mounts = me.mounts();
                let roots: Vec<RootRow> = index::roots(&conn)?
                    .into_iter()
                    .filter(|r| root_ids.as_ref().map(|ids| ids.contains(&r.id)).unwrap_or(true))
                    .collect();
                for root in roots {
                    if task.is_cancelled() {
                        break;
                    }
                    let Some(mount) = mounts.get(&root.volume_id) else { continue };
                    task.detail(volumes::join_rel(mount, &root.rel_path).to_string_lossy().into_owned());
                    let notify = || me.notify_changed(false);
                    match index::scan_root(&mut conn, &root, mount, &exceptions, Some(&task), &notify) {
                        Ok(stats) => log::info!("Indexation de la racine {} : {:?}", root.id, stats),
                        Err(e) => log::warn!("Indexation de la racine {} impossible : {e}", root.id),
                    }
                }
                Ok(())
            })();
            me.notify_changed(true);
            match result {
                Ok(()) => task.finish(),
                Err(e) => task.fail(e.to_string()),
            }
            me.spawn_hash();
        });
    }

    /// Calcul des hash audio manquants, en tâche de fond, un fichier à la fois
    /// (pour ne pas saturer un disque USB).
    pub fn spawn_hash(self: &Arc<Self>) {
        if self.hashing.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), AppError> {
                let conn = me.conn()?;
                let online: Vec<i64> = me.mounts().keys().copied().collect();
                if online.is_empty() {
                    return Ok(());
                }
                let list = online.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
                let total: i64 = conn.query_row(
                    &format!("SELECT count(*) FROM tracks WHERE audio_hash IS NULL AND status = 'ok' AND volume_id IN ({list})"),
                    [],
                    |r| r.get(0),
                )?;
                if total == 0 {
                    return Ok(());
                }
                let task = me.tasks.start("tasks.hash", None);
                task.set_total(total as u64);
                let mut done = 0u64;
                let mut failed: HashSet<i64> = HashSet::new();
                'outer: loop {
                    let mounts = me.mounts();
                    let batch = index::pending_hashes(&conn, &mounts.keys().copied().collect::<Vec<_>>(), 100)?;
                    let batch: Vec<_> = batch.into_iter().filter(|(id, _, _)| !failed.contains(id)).collect();
                    if batch.is_empty() {
                        break;
                    }
                    for (id, vol, rel) in batch {
                        if task.is_cancelled() {
                            break 'outer;
                        }
                        let Some(mount) = mounts.get(&vol) else {
                            failed.insert(id);
                            continue;
                        };
                        let path = volumes::join_rel(mount, &rel);
                        match crate::audio::hash::audio_stream_hash(&path, Some(task.cancel_flag())) {
                            Ok(h) => index::set_hash(&conn, id, Some(&h), None)?,
                            Err(crate::audio::hash::HashError::Cancelled) => break 'outer,
                            Err(e) => {
                                failed.insert(id);
                                if path.exists() {
                                    index::set_hash(&conn, id, None, Some(&e.to_string()))?;
                                }
                            }
                        }
                        done += 1;
                        task.progress(done);
                    }
                }
                task.finish();
                Ok(())
            })();
            if let Err(e) = result {
                log::warn!("Calcul des hash audio interrompu : {e}");
            }
            me.hashing.store(false, Ordering::SeqCst);
            me.notify_changed(true);
        });
    }

    /// Surveille le branchement et le débranchement des disques.
    pub fn start_monitor(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::spawn(move || loop {
            let before: HashSet<i64> = me.mounts().keys().copied().collect();
            match me.refresh_mounts() {
                Ok(appeared) => {
                    let after: HashSet<i64> = me.mounts().keys().copied().collect();
                    if before != after {
                        me.restart_watcher();
                    }
                    if !appeared.is_empty() {
                        let ids: Vec<i64> = me
                            .conn()
                            .and_then(|c| index::roots(&c))
                            .map(|rs| rs.into_iter().filter(|r| appeared.contains(&r.volume_id)).map(|r| r.id).collect())
                            .unwrap_or_default();
                        if !ids.is_empty() {
                            me.spawn_scan(Some(ids));
                        }
                    }
                }
                Err(e) => log::warn!("Détection des disques : {e}"),
            }
            std::thread::sleep(Duration::from_secs(4));
        });
    }

    /// (Re)lance la surveillance des dossiers racines en ligne.
    pub fn restart_watcher(self: &Arc<Self>) {
        let roots = match self.conn().and_then(|c| index::roots(&c)) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Surveillance : {e}");
                return;
            }
        };
        let mounts = self.mounts();
        let (tx, rx) = mpsc::channel::<PathBuf>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                for p in ev.paths {
                    let _ = tx.send(p);
                }
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Surveillance impossible : {e}");
                return;
            }
        };
        for r in &roots {
            if let Some(m) = mounts.get(&r.volume_id) {
                let dir = volumes::join_rel(m, &r.rel_path);
                if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
                    log::warn!("Surveillance de {} impossible : {e}", dir.display());
                }
            }
        }
        if let Ok(mut w) = self.watcher.lock() {
            *w = Some(watcher); // l'ancien est arrêté en étant remplacé
        }
        let me = self.clone();
        std::thread::spawn(move || {
            // Regroupe les événements : on attend 1,5 s de calme avant de traiter.
            loop {
                let Ok(first) = rx.recv() else { return };
                let mut paths: HashSet<PathBuf> = HashSet::from([first]);
                while let Ok(p) = rx.recv_timeout(Duration::from_millis(1500)) {
                    paths.insert(p);
                    if paths.len() > 5000 {
                        break;
                    }
                }
                if let Err(e) = me.handle_fs_changes(paths) {
                    log::warn!("Mise à jour après changement de fichiers : {e}");
                }
            }
        });
    }

    fn handle_fs_changes(self: &Arc<Self>, paths: HashSet<PathBuf>) -> Result<(), AppError> {
        let conn = self.conn()?;
        let exceptions = index::artist_exceptions(&conn)?;
        let mounts = self.mounts();
        let roots = index::roots(&conn)?;
        let mut rescan: HashSet<i64> = HashSet::new();
        for p in paths {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || p.components().any(|c| c.as_os_str() == index::QUARANTINE_DIR) {
                continue;
            }
            // Racine concernée
            let Some((root, mount)) = roots.iter().find_map(|r| {
                let m = mounts.get(&r.volume_id)?;
                let dir = volumes::join_rel(m, &r.rel_path);
                p.starts_with(&dir).then(|| (r.clone(), m.clone()))
            }) else {
                continue;
            };
            let rel = volumes::to_rel_string(p.strip_prefix(&mount).unwrap_or(&p));
            if p.is_dir() {
                rescan.insert(root.id);
            } else if p.is_file() {
                if is_audio_path(&p) {
                    let meta = std::fs::metadata(&p)?;
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let res = crate::tags::read::read_file(&p, &exceptions);
                    index::store(&conn, root.volume_id, Some(root.id), &rel, meta.len() as i64, mtime, res)?;
                }
            } else {
                // Supprimé ou déplacé : le fichier (ou tout le dossier) est marqué manquant.
                conn.execute(
                    "UPDATE tracks SET status = 'missing' WHERE volume_id = ?1 AND (rel_path = ?2 OR substr(rel_path, 1, length(?3)) = ?3)",
                    params![root.volume_id, rel, format!("{rel}/")],
                )?;
                rescan.insert(root.id); // un renommage fait apparaître le nouveau nom ailleurs
            }
        }
        self.notify_changed(true);
        if !rescan.is_empty() {
            self.spawn_scan(Some(rescan.into_iter().collect()));
        } else {
            self.spawn_hash();
        }
        Ok(())
    }
}
