//! Quarantaine (principe 3 : rien n'est supprimé définitivement).
//!
//! Un fichier écarté est déplacé (jamais copié ni effacé) dans « _Quarantaine » à la
//! racine de son disque, en gardant son arborescence d'origine. Le déplacement reste sur
//! le même disque : il est instantané et ne prend pas de place. La fiche du morceau est
//! gardée en base (état « quarantined ») pour pouvoir le remettre en place en un clic.
//! Seul « Vider la quarantaine », après confirmation, envoie les fichiers dans la
//! corbeille du système.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::edit::{EditFailure, EditReport};
use super::index::QUARANTINE_DIR;
use super::{volumes, Library};
use crate::error::AppError;
use crate::history;

/// État d'un morceau pour l'historique : où est le fichier, et s'il est en quarantaine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineState {
    pub volume_id: i64,
    pub rel_path: String,
    pub quarantined: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineItem {
    pub id: i64,
    pub track_id: Option<i64>,
    pub file_name: String,
    pub title: Option<String>,
    pub artists: Option<String>,
    pub orig_rel: String,
    pub quar_rel: String,
    pub volume_id: i64,
    pub volume_label: String,
    pub size: i64,
    pub reason: Option<String>,
    pub added_at: i64,
    pub online: bool,
    pub exists: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineSummary {
    pub count: usize,
    pub size: i64,
    pub offline: usize,
}

fn fail(id: i64, name: &str, code: &str, msg: impl Into<String>) -> EditFailure {
    EditFailure { track_id: id, file_name: name.to_string(), code: code.into(), message: msg.into() }
}

/// Chemin libre dans la quarantaine : « <dossier de bibliothèque>/_Quarantaine/<chemin d'origine> »,
/// avec « (2) », « (3) »… si un fichier du même nom y est déjà.
fn free_target(mount: &Path, root_rel: &str, orig_rel: &str) -> String {
    let inner = if root_rel.is_empty() { orig_rel } else { orig_rel.strip_prefix(&format!("{root_rel}/")).unwrap_or(orig_rel) };
    let base = if root_rel.is_empty() || inner == orig_rel {
        format!("{QUARANTINE_DIR}/{orig_rel}")
    } else {
        format!("{root_rel}/{QUARANTINE_DIR}/{inner}")
    };
    if !volumes::join_rel(mount, &base).exists() {
        return base;
    }
    let (dir, name) = base.rsplit_once('/').unwrap_or(("", &base));
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 2..10_000 {
        let candidate = format!("{dir}/{stem} ({n}){ext}");
        if !volumes::join_rel(mount, &candidate).exists() {
            return candidate;
        }
    }
    format!("{dir}/{stem} ({}){ext}", std::process::id())
}

/// Supprime les dossiers devenus vides dans la quarantaine (jamais en dehors).
fn prune_empty_dirs(mount: &Path, quar_rel: &str) {
    let Some(pos) = quar_rel.split('/').position(|p| p == QUARANTINE_DIR) else { return };
    let qroot: Vec<&str> = quar_rel.split('/').take(pos + 1).collect();
    let root = volumes::join_rel(mount, &qroot.join("/"));
    let mut dir = volumes::join_rel(mount, quar_rel).parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        if !d.starts_with(&root) || d == root {
            break;
        }
        // remove_dir échoue (sans rien effacer) si le dossier n'est pas vide.
        if std::fs::remove_dir(&d).is_err() {
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    let _ = std::fs::remove_dir(&root); // la quarantaine vide disparaît aussi
}

impl Library {
    /// Met des morceaux en quarantaine, dans un nouveau lot d'historique.
    pub fn quarantine(&self, ids: &[i64], reason: &str) -> Result<EditReport, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let batch = history::begin_batch(&conn, "history.quarantine", None)?;
        let mut report = self.quarantine_in(&conn, ids, reason, batch)?;
        if history::drop_if_empty(&conn, batch)? {
            report.batch_id = None;
        }
        self.notify_changed(true);
        Ok(report)
    }

    /// Mise en quarantaine dans un lot déjà ouvert.
    pub(crate) fn quarantine_in(&self, conn: &Connection, ids: &[i64], reason: &str, batch: i64) -> Result<EditReport, AppError> {
        let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
        for &id in ids {
            let row: Option<(i64, String, String, String, Option<String>)> = conn
                .query_row(
                    "SELECT t.volume_id, t.rel_path, t.file_name, t.status, r.rel_path FROM tracks t
                     LEFT JOIN roots r ON r.id = t.root_id AND r.volume_id = t.volume_id WHERE t.id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let Some((vol, rel, name, status, root_rel)) = row else { continue };
            if status == "quarantined" {
                continue;
            }
            let Some(mount) = self.mounts().get(&vol).cloned() else {
                report.failures.push(fail(id, &name, "offline", "disque déconnecté"));
                continue;
            };
            let target = free_target(&mount, root_rel.as_deref().unwrap_or(""), &rel);
            let before = QuarantineState { volume_id: vol, rel_path: rel.clone(), quarantined: false, reason: None };
            let after = QuarantineState { volume_id: vol, rel_path: target, quarantined: true, reason: Some(reason.to_string()) };
            match self.apply_quarantine_state(conn, id, &after) {
                Ok(()) => {
                    history::record(
                        conn,
                        batch,
                        "quarantine",
                        Some(id),
                        &serde_json::to_string(&before).unwrap_or_default(),
                        &serde_json::to_string(&after).unwrap_or_default(),
                    )?;
                    log::info!("Mis en quarantaine : {rel} → {}", after.rel_path);
                    report.ok += 1;
                }
                Err(f) => report.failures.push(f),
            }
        }
        crate::duplicates::prune(conn)?;
        Ok(report)
    }

    /// Place le fichier d'un morceau à l'endroit décrit par `state` (en quarantaine ou non).
    /// Sert à la mise en quarantaine, à la restauration et à l'historique (annuler / rétablir).
    pub(crate) fn apply_quarantine_state(&self, conn: &Connection, id: i64, state: &QuarantineState) -> Result<(), EditFailure> {
        let row: Option<(i64, String, String, i64)> = conn
            .query_row("SELECT volume_id, rel_path, file_name, size FROM tracks WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()
            .map_err(|e| fail(id, "", "internal", e.to_string()))?;
        let Some((vol, rel, name, size)) = row else { return Err(fail(id, "", "internal", "morceau introuvable")) };
        let mount = self.mounts().get(&vol).cloned().ok_or_else(|| fail(id, &name, "offline", "disque déconnecté"))?;
        let from = volumes::join_rel(&mount, &rel);
        let to = volumes::join_rel(&mount, &state.rel_path);
        let moved = rel != state.rel_path;
        if moved {
            if to.exists() {
                return Err(fail(id, &name, "exists", format!("« {} » existe déjà", state.rel_path)));
            }
            if !from.exists() {
                return Err(fail(id, &name, "missing", "fichier introuvable"));
            }
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| fail(id, &name, "io", crate::tags::write::classify_io(e).to_string()))?;
            }
            // Même disque : un simple renommage, instantané, sans copie.
            std::fs::rename(&from, &to).map_err(|e| fail(id, &name, "io", crate::tags::write::classify_io(e).to_string()))?;
        }
        let db = (|| -> Result<(), rusqlite::Error> {
            conn.execute(
                "DELETE FROM tracks WHERE volume_id = ?1 AND rel_path = ?2 AND id != ?3 AND status = 'missing'",
                params![vol, state.rel_path, id],
            )?;
            let status = if state.quarantined { "quarantined" } else { "ok" };
            conn.execute("UPDATE tracks SET rel_path = ?2, status = ?3 WHERE id = ?1", params![id, state.rel_path, status])?;
            if state.quarantined {
                conn.execute(
                    "INSERT INTO quarantine (track_id, volume_id, orig_rel, quar_rel, size, reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(track_id) DO UPDATE SET orig_rel = excluded.orig_rel, quar_rel = excluded.quar_rel,
                        reason = excluded.reason, added_at = unixepoch()",
                    params![id, vol, rel, state.rel_path, size, state.reason],
                )?;
            } else {
                conn.execute("DELETE FROM quarantine WHERE track_id = ?1", [id])?;
            }
            Ok(())
        })();
        if let Err(e) = db {
            if moved {
                let _ = std::fs::rename(&to, &from); // la base n'a pas suivi : retour à la case départ
            }
            return Err(fail(id, &name, "internal", e.to_string()));
        }
        if moved && !state.quarantined {
            prune_empty_dirs(&mount, &rel);
        }
        if moved {
            crate::rekordbox::journal(conn, id, &from, &to);
        }
        Ok(())
    }

    /// Historique : remet le morceau dans l'état enregistré.
    pub(crate) fn restore_quarantine(&self, conn: &Connection, id: i64, state_json: &str) -> Result<(), EditFailure> {
        let state: QuarantineState = serde_json::from_str(state_json).map_err(|e| fail(id, "", "internal", e.to_string()))?;
        self.apply_quarantine_state(conn, id, &state)
    }

    pub fn quarantine_list(&self) -> Result<Vec<QuarantineItem>, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let mounts = self.mounts();
        let mut st = conn.prepare(
            "SELECT q.id, q.track_id, coalesce(t.file_name, ''), t.title, t.artists, q.orig_rel, q.quar_rel, q.volume_id,
                    v.label, q.size, q.reason, q.added_at
             FROM quarantine q JOIN volumes v ON v.id = q.volume_id LEFT JOIN tracks t ON t.id = q.track_id
             ORDER BY q.added_at DESC, q.id DESC",
        )?;
        let rows = st
            .query_map([], |r| {
                let volume_id: i64 = r.get(7)?;
                let quar_rel: String = r.get(6)?;
                let path = mounts.get(&volume_id).map(|m| volumes::join_rel(m, &quar_rel));
                Ok(QuarantineItem {
                    id: r.get(0)?,
                    track_id: r.get(1)?,
                    file_name: r.get(2)?,
                    title: r.get(3)?,
                    artists: r.get(4)?,
                    orig_rel: r.get(5)?,
                    online: path.is_some(),
                    exists: path.as_ref().map(|p| p.exists()).unwrap_or(false),
                    path: path.map(|p| p.to_string_lossy().into_owned()),
                    quar_rel,
                    volume_id,
                    volume_label: r.get(8)?,
                    size: r.get(9)?,
                    reason: r.get(10)?,
                    added_at: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Remet des fichiers à leur emplacement d'origine (nouveau lot d'historique : la
    /// restauration elle-même s'annule).
    pub fn quarantine_restore(&self, entry_ids: &[i64]) -> Result<EditReport, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let batch = history::begin_batch(&conn, "history.restore", None)?;
        let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
        for &qid in entry_ids {
            let row: Option<(Option<i64>, i64, String, String, Option<String>)> = conn
                .query_row("SELECT track_id, volume_id, orig_rel, quar_rel, reason FROM quarantine WHERE id = ?1", [qid], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .optional()?;
            let Some((Some(tid), vol, orig, quar, reason)) = row else { continue };
            let before = QuarantineState { volume_id: vol, rel_path: quar, quarantined: true, reason };
            let after = QuarantineState { volume_id: vol, rel_path: orig, quarantined: false, reason: None };
            match self.apply_quarantine_state(&conn, tid, &after) {
                Ok(()) => {
                    history::record(
                        &conn,
                        batch,
                        "quarantine",
                        Some(tid),
                        &serde_json::to_string(&before).unwrap_or_default(),
                        &serde_json::to_string(&after).unwrap_or_default(),
                    )?;
                    report.ok += 1;
                }
                Err(f) => report.failures.push(f),
            }
        }
        if history::drop_if_empty(&conn, batch)? {
            report.batch_id = None;
        }
        self.notify_changed(true);
        Ok(report)
    }

    /// Nombre et taille des fichiers qui partiraient à la corbeille (pour la confirmation).
    pub fn quarantine_summary(&self, entry_ids: Option<&[i64]>) -> Result<QuarantineSummary, AppError> {
        let items = self.quarantine_list()?;
        let mut s = QuarantineSummary::default();
        for it in items.iter().filter(|i| entry_ids.map(|ids| ids.contains(&i.id)).unwrap_or(true)) {
            if it.online {
                s.count += 1;
                s.size += it.size;
            } else {
                s.offline += 1;
            }
        }
        Ok(s)
    }

    /// « Vider la quarantaine » : les fichiers partent dans la corbeille du système
    /// (après confirmation dans l'interface). Les disques débranchés sont laissés de côté.
    pub fn quarantine_empty(&self, entry_ids: Option<&[i64]>) -> Result<EditReport, AppError> {
        self.quarantine_empty_with(entry_ids, &|paths: &[PathBuf]| trash::delete_all(paths).map_err(|e| e.to_string()))
    }

    pub(crate) fn quarantine_empty_with(
        &self,
        entry_ids: Option<&[i64]>,
        to_trash: &dyn Fn(&[PathBuf]) -> Result<(), String>,
    ) -> Result<EditReport, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let mut report = EditReport::default();
        let items: Vec<QuarantineItem> =
            self.quarantine_list()?.into_iter().filter(|i| entry_ids.map(|ids| ids.contains(&i.id)).unwrap_or(true)).collect();
        for it in items {
            if !it.online {
                report.failures.push(fail(it.track_id.unwrap_or_default(), &it.file_name, "offline", "disque déconnecté"));
                continue;
            }
            let path = PathBuf::from(it.path.clone().unwrap_or_default());
            if it.exists {
                // Un fichier à la fois : un échec n'empêche pas les autres, et la base suit exactement.
                if let Err(e) = to_trash(std::slice::from_ref(&path)) {
                    report.failures.push(fail(it.track_id.unwrap_or_default(), &it.file_name, "trash", e));
                    continue;
                }
                log::info!("Envoyé à la corbeille : {}", path.display());
            }
            conn.execute("DELETE FROM quarantine WHERE id = ?1", [it.id])?;
            if let Some(tid) = it.track_id {
                conn.execute("DELETE FROM tracks WHERE id = ?1 AND status = 'quarantined'", [tid])?;
            }
            if let Some(m) = self.mounts().get(&it.volume_id) {
                prune_empty_dirs(m, &it.quar_rel);
            }
            report.ok += 1;
        }
        self.notify_changed(true);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use crate::library::edit::tests_support::library_on;
    use crate::testdata;

    #[test]
    fn quarantaine_puis_restauration_identique() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(music.join("Techno")).unwrap();
        let original = testdata::write(&music.join("Techno"), "a.wav", &testdata::wav_bytes(1));
        testdata::write(&music, "b.flac", &testdata::flac_bytes(2));
        let bytes_before = std::fs::read(&original).unwrap();
        let (lib, ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let wav: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'a.wav'", [], |r| r.get(0)).unwrap();
        assert_eq!(ids.len(), 2);

        let r = lib.quarantine(&[wav], "duplicate").unwrap();
        assert_eq!(r.ok, 1, "{:?}", r.failures);
        assert!(!original.exists(), "le fichier doit avoir quitté son dossier");
        let list = lib.quarantine_list().unwrap();
        assert_eq!(list.len(), 1);
        let qpath = std::path::PathBuf::from(list[0].path.clone().unwrap());
        assert!(qpath.exists());
        assert!(list[0].quar_rel.ends_with("musique/_Quarantaine/Techno/a.wav"), "arborescence gardée : {}", list[0].quar_rel);
        // Chemins réels : sous macOS /var est un lien vers /private/var, et sous Windows le
        // dossier temporaire peut s'écrire en nom court (RUNNER~1).
        let real = |p: &std::path::Path| std::fs::canonicalize(p).unwrap();
        assert!(
            real(&qpath).starts_with(real(&music)),
            "la quarantaine reste dans le dossier de bibliothèque, sur le même disque : {} / {}",
            qpath.display(),
            music.display()
        );
        // Caché de la bibliothèque, jamais marqué manquant par une nouvelle indexation.
        let visible = crate::library::query::ids(&conn, &Default::default()).unwrap();
        assert!(!visible.contains(&wav));
        let root = crate::library::index::roots(&conn).unwrap().remove(0);
        let mut c2 = crate::db::open_worker(lib.db_path()).unwrap();
        crate::library::index::scan_root(&mut c2, &root, &lib.mounts()[&root.volume_id], &[], None, &|| {}).unwrap();
        let st: String = conn.query_row("SELECT status FROM tracks WHERE id = ?1", [wav], |r| r.get(0)).unwrap();
        assert_eq!(st, "quarantined");

        // Restauration : à sa place, octet pour octet.
        let r = lib.quarantine_restore(&[list[0].id]).unwrap();
        assert_eq!(r.ok, 1, "{:?}", r.failures);
        assert_eq!(std::fs::read(&original).unwrap(), bytes_before);
        assert!(lib.quarantine_list().unwrap().is_empty());
        let st: String = conn.query_row("SELECT status FROM tracks WHERE id = ?1", [wav], |r| r.get(0)).unwrap();
        assert_eq!(st, "ok");

        // Annuler la restauration la remet en quarantaine ; annuler la quarantaine la ramène.
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(!original.exists());
        assert_eq!(lib.quarantine_list().unwrap().len(), 1);
        let first = crate::history::list(&conn, 10).unwrap().into_iter().find(|b| b.label == "history.quarantine").unwrap();
        // Après redémarrage (nouvelle bibliothèque sur la même base) : l'annulation marche toujours.
        let lib2 = crate::library::Library::new(lib.db_path().to_path_buf(), crate::tasks::Tasks::new(None), None);
        lib2.refresh_mounts().unwrap();
        // L'annulation de la restauration a laissé le lot de restauration « annulé » : on reprend dans l'ordre.
        let rep = lib2.apply_history(first.id, true).unwrap();
        assert_eq!(rep.failures.len(), 0, "{:?}", rep.failures);
        assert_eq!(std::fs::read(&original).unwrap(), bytes_before);
        assert!(!dir.path().join("musique").join("_Quarantaine").exists() || lib2.quarantine_list().unwrap().is_empty());
    }

    #[test]
    fn vider_la_quarantaine() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        testdata::write(&music, "a.wav", &testdata::wav_bytes(1));
        testdata::write(&music, "b.wav", &testdata::wav_bytes(2));
        let (lib, ids) = library_on(&dir, &music);
        lib.quarantine(&ids, "manual").unwrap();
        let s = lib.quarantine_summary(None).unwrap();
        assert_eq!(s.count, 2);
        assert!(s.size > 0);
        // La vraie corbeille n'est pas utilisée pendant les tests : on simule son effet.
        let trashed = std::sync::Mutex::new(Vec::new());
        let r = lib
            .quarantine_empty_with(None, &|p: &[std::path::PathBuf]| {
                for f in p {
                    std::fs::remove_file(f).map_err(|e| e.to_string())?;
                    trashed.lock().unwrap().push(f.clone());
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(r.ok, 2);
        assert_eq!(trashed.lock().unwrap().len(), 2);
        assert!(lib.quarantine_list().unwrap().is_empty());
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn nom_deja_pris_en_quarantaine() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        testdata::write(&music, "a.wav", &testdata::wav_bytes(1));
        let (lib, ids) = library_on(&dir, &music);
        // Un fichier du même nom est déjà dans la quarantaine.
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let vol: i64 = conn.query_row("SELECT volume_id FROM tracks WHERE id = ?1", [ids[0]], |r| r.get(0)).unwrap();
        let mount = lib.mounts()[&vol].clone();
        let rel: String = conn.query_row("SELECT rel_path FROM tracks WHERE id = ?1", [ids[0]], |r| r.get(0)).unwrap();
        let occupied = crate::library::volumes::join_rel(&mount, &rel).parent().unwrap().join("_Quarantaine").join("a.wav");
        std::fs::create_dir_all(occupied.parent().unwrap()).unwrap();
        std::fs::write(&occupied, b"autre").unwrap();
        let r = lib.quarantine(&ids, "manual").unwrap();
        assert_eq!(r.ok, 1);
        assert_eq!(std::fs::read(&occupied).unwrap(), b"autre", "jamais d'écrasement");
        assert!(lib.quarantine_list().unwrap()[0].quar_rel.ends_with("a (2).wav"));
    }
}
