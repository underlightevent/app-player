//! Gestion des dossiers : arborescence, création, renommage, déplacement de dossiers
//! et de morceaux. Tout est inscrit dans l'historique ; rien n'est déplacé
//! automatiquement (principe 9), et aucun fichier n'est jamais écrasé.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use super::edit::{EditFailure, EditReport};
use super::{index, volumes, Library};
use crate::error::AppError;
use crate::history::{self, PathState};
use crate::rename::template::{is_reserved, sanitize};
use crate::tags::text::nfc;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirInfo {
    pub volume_id: i64,
    pub name: String,
    pub rel_path: String,
    pub track_count: i64,
    pub has_children: bool,
}

/// État d'un dossier pour l'historique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirState {
    pub volume_id: i64,
    pub rel_path: String,
    /// Le dossier existe (création / suppression d'un dossier vide).
    pub exists: bool,
}

fn prefix(rel: &str) -> String {
    if rel.is_empty() {
        String::new()
    } else {
        format!("{rel}/")
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn count_under(conn: &Connection, volume_id: i64, rel: &str) -> Result<i64, AppError> {
    let p = prefix(rel);
    Ok(conn.query_row(
        "SELECT count(*) FROM tracks WHERE volume_id = ?1 AND status NOT IN ('missing', 'quarantined') AND substr(rel_path, 1, length(?2)) = ?2",
        params![volume_id, p],
        |r| r.get(0),
    )?)
}

fn safe_folder_name(name: &str) -> Result<String, AppError> {
    let n = sanitize(name);
    if n.is_empty() || n == "." || n == ".." || n.starts_with('.') || n == index::QUARANTINE_DIR || is_reserved(&n) {
        return Err(AppError::InvalidSetting("nom de dossier invalide".into()));
    }
    Ok(n)
}

/// Met à jour les chemins en base après le déplacement d'un dossier.
fn move_prefix_in_db(conn: &Connection, volume_id: i64, from: &str, to: &str) -> Result<(), AppError> {
    let (fp, tp) = (prefix(from), prefix(to));
    conn.execute(
        "UPDATE tracks SET rel_path = ?3 || substr(rel_path, length(?2) + 1)
         WHERE volume_id = ?1 AND substr(rel_path, 1, length(?2)) = ?2",
        params![volume_id, fp, tp],
    )?;
    conn.execute(
        "UPDATE roots SET rel_path = CASE WHEN rel_path = ?2 THEN ?4 ELSE ?3 || substr(rel_path, length(?5) + 1) END
         WHERE volume_id = ?1 AND (rel_path = ?2 OR substr(rel_path, 1, length(?5)) = ?5)",
        params![volume_id, from, tp, to, fp],
    )?;
    Ok(())
}

impl Library {
    /// Sous-dossiers d'un dossier (lus sur le disque s'il est branché, sinon d'après la base).
    pub fn list_dirs(&self, volume_id: i64, rel: &str) -> Result<Vec<DirInfo>, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let mut names: Vec<String> = Vec::new();
        if let Some(mount) = self.mounts().get(&volume_id) {
            if let Ok(rd) = std::fs::read_dir(volumes::join_rel(mount, rel)) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') || name == index::QUARANTINE_DIR || name.starts_with('$') {
                        continue;
                    }
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        names.push(name);
                    }
                }
            }
        } else {
            // Disque débranché : arborescence reconstituée depuis les morceaux connus.
            let p = prefix(rel);
            let mut st = conn.prepare(
                "SELECT DISTINCT substr(rel_path, length(?2) + 1) FROM tracks WHERE volume_id = ?1 AND substr(rel_path, 1, length(?2)) = ?2",
            )?;
            let rows = st.query_map(params![volume_id, p], |r| r.get::<_, String>(0))?;
            for r in rows {
                if let Some((first, _)) = r?.split_once('/') {
                    if first != index::QUARANTINE_DIR && !names.iter().any(|n| n == first) {
                        names.push(first.to_string());
                    }
                }
            }
        }
        names.sort_by_key(|n| n.to_lowercase());
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            let child = join(rel, &name);
            let has_children = match self.mounts().get(&volume_id) {
                Some(m) => std::fs::read_dir(volumes::join_rel(m, &child))
                    .map(|rd| rd.flatten().any(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false) && !e.file_name().to_string_lossy().starts_with('.')))
                    .unwrap_or(false),
                None => true,
            };
            out.push(DirInfo { volume_id, track_count: count_under(&conn, volume_id, &child)?, name, rel_path: child, has_children });
        }
        Ok(out)
    }

    pub fn create_dir(&self, volume_id: i64, parent: &str, name: &str) -> Result<EditReport, AppError> {
        let name = safe_folder_name(name)?;
        let mount = self.mounts().get(&volume_id).cloned().ok_or(AppError::Offline)?;
        let rel = join(parent, &name);
        let abs = volumes::join_rel(&mount, &rel);
        if abs.exists() {
            return Err(AppError::InvalidSetting("un dossier de ce nom existe déjà".into()));
        }
        std::fs::create_dir(&abs)?;
        let conn = crate::db::open_worker(self.db_path())?;
        let batch = history::begin_batch(&conn, "history.mkdir", Some(&rel))?;
        let before = DirState { volume_id, rel_path: rel.clone(), exists: false };
        let after = DirState { volume_id, rel_path: rel, exists: true };
        history::record(&conn, batch, "mkdir", None, &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
        self.notify_changed(true);
        Ok(EditReport { batch_id: Some(batch), ok: 1, failures: vec![] })
    }

    /// Renomme un dossier (même parent) ou le déplace (`new_rel` complet).
    pub fn move_dir(&self, volume_id: i64, from: &str, new_rel: &str) -> Result<EditReport, AppError> {
        if from.is_empty() || new_rel == from || new_rel.starts_with(&prefix(from)) {
            return Err(AppError::InvalidSetting("déplacement de dossier impossible".into()));
        }
        let conn = crate::db::open_worker(self.db_path())?;
        self.move_dir_inner(&conn, volume_id, from, new_rel)?;
        let batch = history::begin_batch(&conn, "history.moveDir", Some(new_rel))?;
        let before = PathState { volume_id, rel_path: from.to_string() };
        let after = PathState { volume_id, rel_path: new_rel.to_string() };
        history::record(&conn, batch, "dir_move", None, &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
        self.notify_changed(true);
        Ok(EditReport { batch_id: Some(batch), ok: 1, failures: vec![] })
    }

    fn move_dir_inner(&self, conn: &Connection, volume_id: i64, from: &str, to: &str) -> Result<(), AppError> {
        let mount = self.mounts().get(&volume_id).cloned().ok_or(AppError::Offline)?;
        let src = volumes::join_rel(&mount, from);
        let dst = volumes::join_rel(&mount, to);
        let same = nfc(from).to_lowercase() == nfc(to).to_lowercase();
        if dst.exists() && !same {
            return Err(AppError::InvalidSetting("un dossier de ce nom existe déjà".into()));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Morceaux concernés, pour le journal des chemins (relocalisation dans Rekordbox).
        let moved: Vec<(i64, String)> = {
            let mut st = conn.prepare("SELECT id, rel_path FROM tracks WHERE volume_id = ?1 AND substr(rel_path, 1, length(?2)) = ?2")?;
            let v = st.query_map(params![volume_id, prefix(from)], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            v
        };
        std::fs::rename(&src, &dst)?;
        if let Err(e) = move_prefix_in_db(conn, volume_id, from, to) {
            let _ = std::fs::rename(&dst, &src); // la base n'a pas suivi : on remet le dossier
            return Err(e);
        }
        for (id, rel) in moved {
            let new_rel = format!("{}{}", prefix(to), &rel[prefix(from).len()..]);
            crate::rekordbox::journal(conn, id, &volumes::join_rel(&mount, &rel), &volumes::join_rel(&mount, &new_rel));
        }
        log::info!("Dossier déplacé : {} → {}", src.display(), dst.display());
        Ok(())
    }

    /// Aperçu du déplacement de morceaux vers un dossier (même disque).
    pub fn move_tracks_preview(&self, ids: &[i64], volume_id: i64, target: &str) -> Result<Vec<crate::rename::PlanItem>, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let mount = self.mounts().get(&volume_id).cloned();
        let mut out = Vec::new();
        let mut taken: Vec<String> = Vec::new();
        for &id in ids {
            let (vol, rel, name): (i64, String, String) =
                conn.query_row("SELECT volume_id, rel_path, file_name FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            let fname = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            let dir = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
            let new_rel = join(target, &fname);
            let status = if vol != volume_id {
                "other_disk"
            } else if mount.is_none() {
                "offline"
            } else if dir == target {
                "same"
            } else if taken.iter().any(|t| t.eq_ignore_ascii_case(&new_rel))
                || volumes::join_rel(mount.as_ref().unwrap(), &new_rel).exists()
            {
                "conflict"
            } else {
                "ok"
            };
            if status == "ok" {
                taken.push(new_rel.clone());
            }
            out.push(crate::rename::PlanItem {
                id,
                folder: dir,
                old_name: name,
                new_name: Some(new_rel),
                status: status.into(),
                suffixed: false,
                long_path: false,
                in_rekordbox: crate::rekordbox::in_rekordbox(&conn, id),
            });
        }
        Ok(out)
    }

    /// Déplace des morceaux vers un dossier, en un seul lot annulable.
    pub fn move_tracks(&self, ids: &[i64], volume_id: i64, target: &str) -> Result<EditReport, AppError> {
        let plan = self.move_tracks_preview(ids, volume_id, target)?;
        let conn = crate::db::open_worker(self.db_path())?;
        let mount = self.mounts().get(&volume_id).cloned().ok_or(AppError::Offline)?;
        std::fs::create_dir_all(volumes::join_rel(&mount, target))?;
        let batch = history::begin_batch(&conn, "history.moveTracks", Some(target))?;
        let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
        for it in plan {
            if it.status == "same" {
                continue;
            }
            if it.status != "ok" {
                report.failures.push(EditFailure { track_id: it.id, file_name: it.old_name, code: format!("move_{}", it.status), message: it.status });
                continue;
            }
            let from_rel: String = conn.query_row("SELECT rel_path FROM tracks WHERE id = ?1", [it.id], |r| r.get(0))?;
            let to_rel = it.new_name.clone().unwrap_or_default();
            let from = volumes::join_rel(&mount, &from_rel);
            let to = volumes::join_rel(&mount, &to_rel);
            match crate::rename::move_file(&conn, it.id, volume_id, &from, &to, &to_rel) {
                Ok(()) => {
                    crate::rekordbox::journal(&conn, it.id, &from, &to);
                    let before = PathState { volume_id, rel_path: from_rel };
                    let after = PathState { volume_id, rel_path: to_rel };
                    history::record(&conn, batch, "move", Some(it.id), &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
                    report.ok += 1;
                }
                Err(e) => report.failures.push(EditFailure { track_id: it.id, file_name: it.old_name, code: "rename_io".into(), message: e }),
            }
        }
        if history::drop_if_empty(&conn, batch)? {
            report.batch_id = None;
        }
        self.notify_changed(true);
        Ok(report)
    }

    /// Annulation / rétablissement des opérations sur les dossiers.
    pub(crate) fn restore_dir(&self, conn: &Connection, kind: &str, state_json: &str, other_json: &str) -> Result<(), EditFailure> {
        let fail = |m: String| EditFailure { track_id: 0, file_name: String::new(), code: "rename_io".into(), message: m };
        match kind {
            "mkdir" => {
                let s: DirState = serde_json::from_str(state_json).map_err(|e| fail(e.to_string()))?;
                let mount = self.mounts().get(&s.volume_id).cloned().ok_or_else(|| fail("disque déconnecté".into()))?;
                let abs = volumes::join_rel(&mount, &s.rel_path);
                if s.exists {
                    std::fs::create_dir_all(&abs).map_err(|e| fail(e.to_string()))
                } else {
                    // On ne supprime qu'un dossier vide : jamais de fichier perdu.
                    std::fs::remove_dir(&abs).map_err(|_| fail("le dossier n'est plus vide".into()))
                }
            }
            "dir_move" => {
                let target: PathState = serde_json::from_str(state_json).map_err(|e| fail(e.to_string()))?;
                let current: PathState = serde_json::from_str(other_json).map_err(|e| fail(e.to_string()))?;
                self.move_dir_inner(conn, current.volume_id, &current.rel_path, &target.rel_path).map_err(|e| fail(e.to_string()))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::library::edit::tests_support::library_on;

    #[test]
    fn dossiers_creer_renommer_deplacer_annuler() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(music.join("Techno")).unwrap();
        crate::testdata::write(&music.join("Techno"), "a.wav", &crate::testdata::wav_bytes(1));
        crate::testdata::write(&music, "b.mp3", &crate::testdata::mp3_bytes());
        let (lib, ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let (vol, root_rel): (i64, String) = conn.query_row("SELECT volume_id, rel_path FROM roots", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let rel = |s: &str| format!("{root_rel}/{s}");

        // Arborescence
        let dirs = lib.list_dirs(vol, &root_rel).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].name, "Techno");
        assert_eq!(dirs[0].track_count, 1);

        // Création puis annulation
        let r = lib.create_dir(vol, &root_rel, "Nouveau: dossier?").unwrap();
        assert!(music.join("Nouveau - dossier").is_dir());
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(!music.join("Nouveau - dossier").exists());

        // Renommage d'un dossier : les chemins des morceaux suivent, puis annulation.
        let r = lib.move_dir(vol, &rel("Techno"), &rel("Techno Mélodique")).unwrap();
        assert!(music.join("Techno Mélodique").join("a.wav").exists());
        let p: String = conn.query_row("SELECT rel_path FROM tracks WHERE file_name = 'a.wav'", [], |r| r.get(0)).unwrap();
        assert!(p.ends_with("Techno Mélodique/a.wav"), "{p}");
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(music.join("Techno").join("a.wav").exists());

        // Déplacement de morceaux vers un dossier, sans écraser, puis annulation.
        let b: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'b.mp3'", [], |r| r.get(0)).unwrap();
        crate::testdata::write(&music.join("Techno"), "b.mp3", b"homonyme a ne pas ecraser");
        let preview = lib.move_tracks_preview(&[b], vol, &rel("Techno")).unwrap();
        assert_eq!(preview[0].status, "conflict");
        std::fs::remove_file(music.join("Techno").join("b.mp3")).unwrap();
        let r = lib.move_tracks(&[b], vol, &rel("Techno")).unwrap();
        assert_eq!(r.ok, 1, "{:?}", r.failures);
        assert!(music.join("Techno").join("b.mp3").exists() && !music.join("b.mp3").exists());
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(music.join("b.mp3").exists());
        let _ = ids;
    }
}
