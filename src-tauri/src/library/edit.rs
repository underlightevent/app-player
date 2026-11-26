//! Modification des tags d'un ou plusieurs morceaux, avec historique et annulation.

use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::{index, volumes, Library};
use crate::error::AppError;
use crate::history::{self, TagState};
use crate::settings;
use crate::tags::read::read_file;
use crate::tags::write::{self, Changes, WriteError, WriteSettings};
use crate::tags::TrackTags;
use crate::tasks::TaskHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditFailure {
    pub track_id: i64,
    pub file_name: String,
    /// Code pour l'interface (« read_only », « in_use », « offline »…).
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditReport {
    pub batch_id: Option<i64>,
    pub ok: usize,
    pub failures: Vec<EditFailure>,
}

/// Origine d'une modification : saisie de l'utilisateur (verrouille les champs) ou automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Manual,
    Automatic,
}

pub fn write_settings(conn: &Connection) -> WriteSettings {
    let s = settings::get_all(conn).unwrap_or_default();
    WriteSettings {
        version_in_title: s.get("version_in_title").map(|v| v == "true").unwrap_or(true),
        artist_separator: match s.get("artist_separator").map(String::as_str) {
            Some("ampersand") => "&".into(),
            Some("semicolon") => ";".into(),
            _ => ",".into(),
        },
        key_camelot: s.get("key_notation").map(|v| v != "musical").unwrap_or(true),
        bpm_integer: s.get("bpm_decimals").map(|v| v != "true").unwrap_or(true),
        single_cover: s.get("single_cover").map(|v| v != "false").unwrap_or(true),
    }
}

struct TrackLoc {
    volume_id: i64,
    root_id: Option<i64>,
    rel_path: String,
    file_name: String,
}

fn locate(conn: &Connection, id: i64) -> Result<Option<TrackLoc>, AppError> {
    Ok(conn
        .query_row("SELECT volume_id, root_id, rel_path, file_name FROM tracks WHERE id = ?1", [id], |r| {
            Ok(TrackLoc { volume_id: r.get(0)?, root_id: r.get(1)?, rel_path: r.get(2)?, file_name: r.get(3)? })
        })
        .optional()?)
}

fn locked_of(conn: &Connection, id: i64) -> Result<Vec<String>, AppError> {
    let mut st = conn.prepare("SELECT field FROM field_state WHERE track_id = ?1 AND locked = 1")?;
    let v = st.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(v)
}

/// Fixe l'état des cadenas pour `fields` : verrouillés si présents dans `locked`.
pub fn set_locks(conn: &Connection, id: i64, fields: &[&str], locked: &[String], source: &str) -> Result<(), AppError> {
    for f in fields {
        let on = locked.iter().any(|l| l == f);
        conn.execute(
            "INSERT INTO field_state (track_id, field, locked, source) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(track_id, field) DO UPDATE SET locked = excluded.locked, source = excluded.source",
            params![id, f, on, source],
        )?;
    }
    Ok(())
}

/// Valeurs actuelles des champs concernés (pour pouvoir revenir en arrière).
fn inverse(current: &TrackTags, c: &Changes) -> Changes {
    Changes {
        title: c.title.as_ref().map(|_| current.title.clone()),
        version: c.version.as_ref().map(|_| current.version.clone()),
        artists: c.artists.as_ref().map(|_| current.artists.clone()),
        album: c.album.as_ref().map(|_| current.album.clone()),
        year: c.year.map(|_| current.year),
        genre: c.genre.as_ref().map(|_| current.genre.clone()),
        bpm: c.bpm.map(|_| current.bpm),
        key: c.key.as_ref().map(|_| current.key.clone()),
        rating: c.rating.map(|_| current.rating),
        cover: None,
        sync_wav: false,
    }
}

fn failure(loc: Option<&TrackLoc>, id: i64, code: &str, message: String) -> EditFailure {
    EditFailure {
        track_id: id,
        file_name: loc.map(|l| l.file_name.clone()).unwrap_or_default(),
        code: code.into(),
        message,
    }
}

/// Noms d'artistes contenant un séparateur : l'utilisateur les a gardés entiers,
/// on les ajoute à la liste d'exceptions pour ne plus les découper.
fn learn_exceptions(conn: &Connection, artists: &[String]) -> Result<(), AppError> {
    for a in artists {
        let lower = format!(" {} ", a.to_lowercase());
        if [" & ", ", ", " x ", " + ", " vs ", " feat"].iter().any(|s| lower.contains(s)) {
            conn.execute("INSERT OR IGNORE INTO artist_exceptions (name) VALUES (?1)", [a])?;
        }
    }
    Ok(())
}

impl Library {
    pub fn track_path(&self, conn: &Connection, id: i64) -> Result<Option<PathBuf>, AppError> {
        let Some(loc) = locate(conn, id)? else { return Ok(None) };
        Ok(self.mounts().get(&loc.volume_id).map(|m| volumes::join_rel(m, &loc.rel_path)))
    }

    /// Applique les mêmes `changes` aux morceaux `ids`, dans un nouveau lot d'historique.
    pub fn edit(
        &self,
        ids: &[i64],
        changes: Changes,
        source: Source,
        label: &str,
        task: Option<&TaskHandle>,
    ) -> Result<EditReport, AppError> {
        let items: Vec<(i64, Changes)> = ids.iter().map(|id| (*id, changes.clone())).collect();
        self.edit_each(items, source, label, task)
    }

    /// Applique à chaque morceau ses propres changements, dans un seul lot d'historique
    /// (nettoyage, tags depuis le nom de fichier…).
    pub fn edit_each(
        &self,
        items: Vec<(i64, Changes)>,
        source: Source,
        label: &str,
        task: Option<&TaskHandle>,
    ) -> Result<EditReport, AppError> {
        let items: Vec<(i64, Changes)> =
            items.into_iter().map(|(id, c)| (id, c.normalized())).filter(|(_, c)| !c.is_empty()).collect();
        if items.is_empty() {
            return Ok(EditReport::default());
        }
        let conn = db_conn(self)?;
        let batch = history::begin_batch(&conn, label, None)?;
        let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
        if let Some(t) = task {
            t.set_total(items.len() as u64);
        }
        for (n, (id, changes)) in items.iter().enumerate() {
            let id = *id;
            if task.map(|t| t.is_cancelled()).unwrap_or(false) {
                break;
            }
            if source == Source::Manual {
                if let Some(a) = &changes.artists {
                    learn_exceptions(&conn, a)?;
                }
            }
            match self.edit_one(&conn, id, changes, source, batch) {
                Ok(()) => report.ok += 1,
                Err(f) => report.failures.push(f),
            }
            if let Some(t) = task {
                t.progress(n as u64 + 1);
            }
        }
        if history::drop_if_empty(&conn, batch)? {
            report.batch_id = None;
        }
        self.notify_changed(true);
        Ok(report)
    }

    fn edit_one(&self, conn: &Connection, id: i64, changes: &Changes, source: Source, batch: i64) -> Result<(), EditFailure> {
        let loc = locate(conn, id).map_err(|e| failure(None, id, "internal", e.to_string()))?;
        let Some(loc) = loc else { return Err(failure(None, id, "internal", "morceau introuvable".into())) };
        let Some(mount) = self.mounts().get(&loc.volume_id).cloned() else {
            return Err(failure(Some(&loc), id, "offline", "disque déconnecté".into()));
        };
        let path = volumes::join_rel(&mount, &loc.rel_path);
        let locked = locked_of(conn, id).map_err(|e| failure(Some(&loc), id, "internal", e.to_string()))?;

        // Une modification automatique ne touche jamais un champ verrouillé (principe 6).
        let mut changes = changes.clone();
        if source == Source::Automatic {
            strip_locked(&mut changes, &locked);
            if changes.is_empty() {
                return Ok(());
            }
        }

        let exceptions = index::artist_exceptions(conn).unwrap_or_default();
        let current = read_file(&path, &exceptions).map_err(|e| failure(Some(&loc), id, "unreadable", e.to_string()))?;
        let fields = changes.fields();

        // État avant (pour l'annulation), pochette comprise.
        let mut before = TagState { changes: inverse(&current.tags, &changes), cover: None, locked: vec![] };
        before.locked = locked.iter().filter(|l| fields.contains(&l.as_str())).cloned().collect();
        if changes.cover.is_some() {
            let old = write::current_cover(&path).map_err(|e| failure(Some(&loc), id, e.code(), e.to_string()))?;
            before.cover = Some(match old {
                Some(c) => Some(history::put_blob(conn, &c).map_err(|e| failure(Some(&loc), id, "internal", e.to_string()))?),
                None => None,
            });
        }

        let ws = write_settings(conn);
        let written = write::write_tags(&path, &current.tags, &changes, &ws, &exceptions)
            .map_err(|e: WriteError| failure(Some(&loc), id, e.code(), e.to_string()))?;

        // Base de données : on lève les cadenas des champs modifiés le temps de relire le fichier.
        let after_locked: Vec<String> = match source {
            Source::Manual => fields.iter().filter(|f| **f != "cover").map(|f| f.to_string()).chain(locked.iter().filter(|l| !fields.contains(&l.as_str())).cloned()).collect(),
            Source::Automatic => locked.clone(),
        };
        let res = (|| -> Result<(), AppError> {
            set_locks(conn, id, &fields, &[], "file")?;
            index::store(conn, loc.volume_id, loc.root_id, &loc.rel_path, written.size as i64, written.mtime, Ok(written.info))?;
            let src = if source == Source::Manual { "manual" } else { "auto" };
            set_locks(conn, id, &fields, &after_locked, src)?;
            let mut after = TagState { changes: changes.clone(), cover: None, locked: vec![] };
            after.changes.cover = None;
            after.locked = after_locked.iter().filter(|l| fields.contains(&l.as_str())).cloned().collect();
            if let Some(c) = &changes.cover {
                after.cover = Some(match c {
                    Some(cd) => Some(history::put_blob(conn, cd)?),
                    None => None,
                });
            }
            history::record(
                conn,
                batch,
                "tags",
                Some(id),
                &serde_json::to_string(&before).unwrap_or_default(),
                &serde_json::to_string(&after).unwrap_or_default(),
            )?;
            Ok(())
        })();
        res.map_err(|e| failure(Some(&loc), id, "internal", e.to_string()))?;
        log::info!("Tags modifiés : {} ({:?})", path.display(), fields);
        Ok(())
    }

    /// Remet un morceau dans l'état enregistré (`state`), sans créer de nouveau lot.
    fn restore_tags(&self, conn: &Connection, id: i64, state: &TagState) -> Result<(), EditFailure> {
        let loc = locate(conn, id).map_err(|e| failure(None, id, "internal", e.to_string()))?;
        let Some(loc) = loc else { return Err(failure(None, id, "internal", "morceau introuvable".into())) };
        let Some(mount) = self.mounts().get(&loc.volume_id).cloned() else {
            return Err(failure(Some(&loc), id, "offline", "disque déconnecté".into()));
        };
        let path = volumes::join_rel(&mount, &loc.rel_path);
        let mut changes = state.changes.clone();
        if let Some(cover) = &state.cover {
            changes.cover = Some(match cover {
                Some(hash) => Some(
                    history::get_blob(conn, hash)
                        .map_err(|e| failure(Some(&loc), id, "internal", e.to_string()))?
                        .ok_or_else(|| failure(Some(&loc), id, "internal", "pochette d'origine introuvable".into()))?,
                ),
                None => None,
            });
        }
        let fields = changes.fields();
        let exceptions = index::artist_exceptions(conn).unwrap_or_default();
        let current = read_file(&path, &exceptions).map_err(|e| failure(Some(&loc), id, "unreadable", e.to_string()))?;
        let ws = write_settings(conn);
        let written = write::write_tags(&path, &current.tags, &changes, &ws, &exceptions)
            .map_err(|e| failure(Some(&loc), id, e.code(), e.to_string()))?;
        (|| -> Result<(), AppError> {
            set_locks(conn, id, &fields, &[], "file")?;
            index::store(conn, loc.volume_id, loc.root_id, &loc.rel_path, written.size as i64, written.mtime, Ok(written.info))?;
            set_locks(conn, id, &fields, &state.locked, if state.locked.is_empty() { "file" } else { "manual" })?;
            Ok(())
        })()
        .map_err(|e| failure(Some(&loc), id, "internal", e.to_string()))
    }

    /// Annule (`undo = true`) ou rétablit un lot.
    pub fn apply_history(&self, batch: i64, undo: bool) -> Result<EditReport, AppError> {
        let conn = db_conn(self)?;
        let mut ops = history::ops(&conn, batch)?;
        if undo {
            ops.reverse();
        }
        let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
        for op in ops {
            let state_json = if undo { &op.before } else { &op.after };
            let result = match op.kind.as_str() {
                "tags" => {
                    let state: TagState = serde_json::from_str(state_json).map_err(|e| AppError::InvalidSetting(e.to_string()))?;
                    self.restore_tags(&conn, op.track_id.unwrap_or_default(), &state)
                }
                "move" => self.restore_path(&conn, op.track_id.unwrap_or_default(), state_json),
                "mkdir" | "dir_move" => {
                    let other = if undo { &op.after } else { &op.before };
                    self.restore_dir(&conn, &op.kind, state_json, other)
                }
                _ => Ok(()),
            };
            match result {
                Ok(()) => report.ok += 1,
                Err(f) => report.failures.push(f),
            }
        }
        // Le lot change d'état si au moins une opération a réussi (les échecs sont signalés).
        if report.ok > 0 || report.failures.is_empty() {
            history::set_undone(&conn, batch, undo)?;
        }
        self.notify_changed(true);
        Ok(report)
    }

    /// Verrouille ou déverrouille un champ pour plusieurs morceaux (sans toucher aux fichiers).
    pub fn set_lock(&self, ids: &[i64], field: &str, locked: bool) -> Result<(), AppError> {
        let conn = db_conn(self)?;
        for &id in ids {
            let mut current = locked_of(&conn, id)?;
            current.retain(|f| f != field);
            if locked {
                current.push(field.to_string());
            }
            set_locks(&conn, id, &[field], &current, if locked { "manual" } else { "file" })?;
        }
        self.notify_changed(true);
        Ok(())
    }

    fn restore_path(&self, conn: &Connection, id: i64, state: &str) -> Result<(), EditFailure> {
        self.restore_path_state(conn, id, state)
    }
}

fn strip_locked(c: &mut Changes, locked: &[String]) {
    let is = |f: &str| locked.iter().any(|l| l == f);
    if is("title") {
        c.title = None;
    }
    if is("version") {
        c.version = None;
    }
    if is("artists") {
        c.artists = None;
    }
    if is("album") {
        c.album = None;
    }
    if is("year") {
        c.year = None;
    }
    if is("genre") {
        c.genre = None;
    }
    if is("bpm") {
        c.bpm = None;
    }
    if is("key") {
        c.key = None;
    }
    if is("rating") {
        c.rating = None;
    }
    if is("cover") {
        c.cover = None;
    }
}

fn db_conn(lib: &Library) -> Result<Connection, AppError> {
    crate::db::open_worker(lib.db_path())
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    /// Bibliothèque de test sur le dossier `music` : base dans `dir/data`, indexation faite.
    pub fn library_on(dir: &tempfile::TempDir, music: &std::path::Path) -> (std::sync::Arc<Library>, Vec<i64>) {
        let (conn, db_path) = crate::db::open(&dir.path().join("data")).unwrap();
        drop(conn);
        let lib = Library::new(db_path, crate::tasks::Tasks::new(None), None);
        let (vol, rel) = volumes::locate(music).unwrap();
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let vol_id = index::upsert_volume(&conn, &vol).unwrap();
        let root = index::add_root(&conn, vol_id, &rel, "library").unwrap();
        drop(conn);
        lib.refresh_mounts().unwrap();
        let mut conn = crate::db::open_worker(lib.db_path()).unwrap();
        let row = index::RootRow { id: root, volume_id: vol_id, rel_path: rel };
        index::scan_root(&mut conn, &row, &vol.mount, &[], None, &|| {}).unwrap();
        let ids: Vec<i64> = conn
            .prepare("SELECT id FROM tracks WHERE status = 'ok' ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        (lib, ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::write::CoverData;
    use crate::testdata;

    /// Bibliothèque de test : une base sur disque, un dossier avec un fichier de chaque format.
    pub fn setup() -> (tempfile::TempDir, std::sync::Arc<Library>, Vec<i64>) {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        testdata::all_formats(&music);
        let (lib, ids) = tests_support::library_on(&dir, &music);
        (dir, lib, ids)
    }

    fn title_of(lib: &Library, id: i64) -> (Option<String>, String) {
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let db: Option<String> = conn.query_row("SELECT title FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        let path = lib.track_path(&conn, id).unwrap().unwrap();
        let file = read_file(&path, &[]).unwrap().tags.title.unwrap_or_default();
        (db, file)
    }

    #[test]
    fn modifier_puis_annuler_un_lot_meme_apres_redemarrage() {
        let (dir, lib, ids) = setup();
        let c = Changes { title: Some(Some("Nouveau titre".into())), rating: Some(5), ..Default::default() };
        let report = lib.edit(&ids, c, Source::Manual, "history.editTags", None).unwrap();
        assert_eq!(report.ok, 4, "{:?}", report.failures);
        for &id in &ids {
            assert_eq!(title_of(&lib, id), (Some("Nouveau titre".into()), "Nouveau titre".into()));
        }
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        assert_eq!(locked_of(&conn, ids[0]).unwrap().len(), 2, "titre et note verrouillés");
        let batch = report.batch_id.unwrap();
        drop(conn);

        // « Redémarrage » : une nouvelle instance relit l'historique depuis la base.
        let lib2 = Library::new(lib.db_path().to_path_buf(), crate::tasks::Tasks::new(None), None);
        lib2.refresh_mounts().unwrap();
        let conn = crate::db::open_worker(lib2.db_path()).unwrap();
        assert_eq!(history::last_undoable(&conn).unwrap(), Some(batch));
        let undo = lib2.apply_history(batch, true).unwrap();
        assert_eq!(undo.ok, 4, "{:?}", undo.failures);
        for &id in &ids {
            let (db, file) = title_of(&lib2, id);
            assert_eq!(file, "", "titre d'origine (vide) non restauré");
            assert_eq!(db, None);
        }
        assert!(locked_of(&conn, ids[0]).unwrap().is_empty(), "cadenas non restaurés");
        // Rétablir
        let redo = lib2.apply_history(batch, false).unwrap();
        assert_eq!(redo.ok, 4);
        assert_eq!(title_of(&lib2, ids[0]).1, "Nouveau titre");
        drop(dir);
    }

    #[test]
    fn pochette_remplacee_puis_restauree() {
        let (_dir, lib, ids) = setup();
        let first = CoverData { mime: "image/jpeg".into(), data: crate::artwork::prepare(&crate::artwork::test_png(600, 600), 1000).unwrap().cover.data };
        let second = crate::artwork::prepare(&crate::artwork::test_png(900, 880), 1000).unwrap().cover;
        let id = ids[0];
        lib.edit(&[id], Changes { cover: Some(Some(first.clone())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let r = lib.edit(&[id], Changes { cover: Some(Some(second.clone())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let path = lib.track_path(&conn, id).unwrap().unwrap();
        assert_eq!(write::current_cover(&path).unwrap().unwrap().data, second.data);
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert_eq!(write::current_cover(&path).unwrap().unwrap().data, first.data, "pochette d'origine non restaurée à l'identique");
    }

    #[test]
    fn automatisation_ne_touche_pas_un_champ_verrouille() {
        let (_dir, lib, ids) = setup();
        let id = ids[1];
        lib.edit(&[id], Changes { genre: Some(Some("Choisi à la main".into())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        lib.edit(&[id], Changes { genre: Some(Some("Proposé en ligne".into())), year: Some(Some(1999)), ..Default::default() }, Source::Automatic, "x", None).unwrap();
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let (genre, year): (Option<String>, Option<i64>) =
            conn.query_row("SELECT genre, year FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(genre.as_deref(), Some("Choisi à la main"));
        assert_eq!(year, Some(1999));
        // Déverrouillé : l'automatisation peut l'écrire.
        lib.set_lock(&[id], "genre", false).unwrap();
        lib.edit(&[id], Changes { genre: Some(Some("Proposé en ligne".into())), ..Default::default() }, Source::Automatic, "x", None).unwrap();
        let genre: Option<String> = conn.query_row("SELECT genre FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(genre.as_deref(), Some("Proposé en ligne"));
    }

    #[test]
    fn disque_deconnecte_lecture_seule() {
        let (_dir, lib, ids) = setup();
        // On simule le débranchement : plus aucun disque en ligne.
        lib.mounts.write().unwrap().clear();
        let r = lib.edit(&ids[..1], Changes { rating: Some(3), ..Default::default() }, Source::Manual, "x", None).unwrap();
        assert_eq!(r.ok, 0);
        assert_eq!(r.failures[0].code, "offline");
        assert_eq!(r.batch_id, None, "pas de lot vide dans l'historique");
    }

    #[test]
    fn exception_apprise() {
        let (_dir, lib, ids) = setup();
        lib.edit(&ids[..1], Changes { artists: Some(vec!["Duo & Compagnie".into(), "Autre".into()]), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        assert!(index::artist_exceptions(&conn).unwrap().iter().any(|e| e == "Duo & Compagnie"));
        let artists: String = conn.query_row("SELECT artists FROM tracks WHERE id = ?1", [ids[0]], |r| r.get(0)).unwrap();
        assert_eq!(artists, "Duo & Compagnie, Autre");
    }
}
