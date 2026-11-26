//! Renommage des fichiers selon un modèle, avec aperçu, gestion des conflits et annulation.
//! Un fichier n'est jamais écrasé : si le nom visé existe déjà, il est signalé.

pub mod clean;
pub mod ops;
pub mod parse;
pub mod template;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::history::{self, PathState};
use crate::library::edit::{EditFailure, EditReport};
use crate::library::{volumes, Library};
use crate::tags::text::nfc;
use template::{is_reserved, render, sanitize, Values, WARN_PATH_CHARS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    /// Ajouter « (2) », « (3) »…
    Suffix,
    /// Ne pas renommer ce fichier.
    Skip,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: i64,
    pub folder: String,
    pub old_name: String,
    pub new_name: Option<String>,
    /// ok, same, conflict, reserved, empty, offline, missing
    pub status: String,
    /// Le nom a reçu un suffixe pour éviter un conflit.
    pub suffixed: bool,
    /// Chemin complet de plus de 250 caractères.
    pub long_path: bool,
}

struct Row {
    id: i64,
    volume_id: i64,
    rel_path: String,
    status: String,
    values: Values,
}

fn load_rows(conn: &Connection, ids: &[i64], key_camelot: bool) -> Result<Vec<Row>, AppError> {
    let mut out = Vec::new();
    let mut st = conn.prepare(
        "SELECT id, volume_id, rel_path, file_name, status, title, version, album, year, genre, bpm, musical_key, rating
         FROM tracks WHERE id = ?1",
    )?;
    let mut ast = conn.prepare("SELECT name FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
    for &id in ids {
        let row = st
            .query_row([id], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    volume_id: r.get(1)?,
                    rel_path: r.get(2)?,
                    status: r.get(4)?,
                    values: Values {
                        artists: vec![],
                        title: r.get(5)?,
                        version: r.get(6)?,
                        album: r.get(7)?,
                        year: r.get(8)?,
                        genre: r.get(9)?,
                        bpm: r.get(10)?,
                        key: r.get(11)?,
                        rating: r.get(12)?,
                        key_camelot,
                    },
                })
            })
            .optional()?;
        if let Some(mut row) = row {
            row.values.artists = ast.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
            out.push(row);
        }
    }
    Ok(out)
}

fn split_rel(rel: &str) -> (&str, &str) {
    match rel.rfind('/') {
        Some(i) => (&rel[..i], &rel[i + 1..]),
        None => ("", rel),
    }
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Clé de comparaison des noms (Windows et macOS ne distinguent pas majuscules et minuscules).
fn name_key(volume: i64, dir: &str, name: &str) -> (i64, String, String) {
    (volume, nfc(dir).to_lowercase(), nfc(name).to_lowercase())
}

fn with_suffix(stem: &str, ext: &str, n: usize) -> String {
    format!("{stem} ({n}).{ext}")
}

/// Calcule l'aperçu du renommage.
pub fn plan(lib: &Library, conn: &Connection, ids: &[i64], tpl: &str, on_conflict: OnConflict) -> Result<Vec<PlanItem>, AppError> {
    template::validate(tpl).map_err(|e| AppError::InvalidSetting(e.to_string()))?;
    let key_camelot = crate::library::edit::write_settings(conn).key_camelot;
    let rows = load_rows(conn, ids, key_camelot)?;
    let mounts = lib.mounts();
    // Noms actuels des fichiers concernés : ils seront libérés par le renommage.
    let sources: HashSet<(i64, String, String)> = rows
        .iter()
        .map(|r| {
            let (dir, name) = split_rel(&r.rel_path);
            name_key(r.volume_id, dir, name)
        })
        .collect();
    let mut taken: HashSet<(i64, String, String)> = HashSet::new();
    let mut items = Vec::with_capacity(rows.len());

    for r in &rows {
        let (dir, old_name) = split_rel(&r.rel_path);
        let mut item = PlanItem {
            id: r.id,
            folder: dir.to_string(),
            old_name: old_name.to_string(),
            new_name: None,
            status: "ok".into(),
            suffixed: false,
            long_path: false,
        };
        let Some(mount) = mounts.get(&r.volume_id) else {
            item.status = "offline".into();
            items.push(item);
            continue;
        };
        if r.status == "missing" {
            item.status = "missing".into();
            items.push(item);
            continue;
        }
        let ext = Path::new(old_name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let stem = match render(tpl, &r.values).map_err(|e| AppError::InvalidSetting(e.to_string()))? {
            Some(s) => sanitize(&s),
            None => String::new(),
        };
        if stem.is_empty() {
            item.status = "empty".into();
            items.push(item);
            continue;
        }
        if is_reserved(&stem) {
            item.status = "reserved".into();
            item.new_name = Some(format!("{stem}.{ext}"));
            items.push(item);
            continue;
        }
        let mut name = format!("{stem}.{ext}");
        if name == old_name {
            item.status = "same".into();
            item.new_name = Some(name.clone());
            taken.insert(name_key(r.volume_id, dir, &name));
            items.push(item);
            continue;
        }
        let dir_abs = volumes::join_rel(mount, dir);
        let occupied = |n: &str, taken: &HashSet<(i64, String, String)>| {
            let k = name_key(r.volume_id, dir, n);
            if taken.contains(&k) {
                return true;
            }
            // Un fichier existant qui n'est pas lui-même et qui ne va pas être renommé.
            let same_file = nfc(n).to_lowercase() == nfc(old_name).to_lowercase();
            !same_file && !sources.contains(&k) && dir_abs.join(n).exists()
        };
        if occupied(&name, &taken) {
            match on_conflict {
                OnConflict::Skip => {
                    item.status = "conflict".into();
                    item.new_name = Some(name);
                    items.push(item);
                    continue;
                }
                OnConflict::Suffix => {
                    let mut n = 2;
                    while occupied(&with_suffix(&stem, &ext, n), &taken) {
                        n += 1;
                    }
                    name = with_suffix(&stem, &ext, n);
                    item.suffixed = true;
                }
            }
        }
        taken.insert(name_key(r.volume_id, dir, &name));
        item.long_path = dir_abs.join(&name).to_string_lossy().chars().count() > WARN_PATH_CHARS;
        item.new_name = Some(name);
        items.push(item);
    }
    Ok(items)
}

struct Move {
    id: i64,
    volume_id: i64,
    from_rel: String,
    to_rel: String,
    from_abs: PathBuf,
    to_abs: PathBuf,
}

/// Déplace un fichier sans jamais en écraser un autre, puis met la base à jour.
pub fn move_file(conn: &Connection, id: i64, volume_id: i64, from_abs: &Path, to_abs: &Path, to_rel: &str) -> Result<(), String> {
    let same_file = from_abs.to_string_lossy().to_lowercase() == to_abs.to_string_lossy().to_lowercase();
    if to_abs.exists() && !same_file {
        return Err(format!("« {} » existe déjà", to_abs.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
    }
    if !from_abs.exists() {
        return Err("fichier introuvable".into());
    }
    std::fs::rename(from_abs, to_abs).map_err(|e| crate::tags::write::classify_io(e).to_string())?;
    let file_name = nfc(to_rel.rsplit('/').next().unwrap_or(to_rel));
    let res = (|| -> Result<(), rusqlite::Error> {
        // Une ancienne fiche « manquante » au même endroit est remplacée.
        conn.execute(
            "DELETE FROM tracks WHERE volume_id = ?1 AND rel_path = ?2 AND id != ?3 AND status = 'missing'",
            params![volume_id, to_rel, id],
        )?;
        conn.execute("UPDATE tracks SET rel_path = ?2, file_name = ?3, status = 'ok' WHERE id = ?1", params![id, to_rel, file_name])?;
        Ok(())
    })();
    if let Err(e) = res {
        // La base n'a pas suivi : on remet le fichier à sa place.
        let _ = std::fs::rename(to_abs, from_abs);
        return Err(e.to_string());
    }
    Ok(())
}

impl Library {
    /// Renomme les fichiers selon le modèle (l'aperçu est recalculé ici, jamais repris tel quel).
    pub fn rename(&self, ids: &[i64], tpl: &str, on_conflict: OnConflict) -> Result<EditReport, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let items = plan(self, &conn, ids, tpl, on_conflict)?;
        let mounts = self.mounts();
        let mut report = EditReport::default();
        let mut moves: Vec<Move> = Vec::new();
        {
            let mut st = conn.prepare("SELECT volume_id, rel_path FROM tracks WHERE id = ?1")?;
            for it in &items {
                if it.status != "ok" {
                    if !["same"].contains(&it.status.as_str()) {
                        report.failures.push(EditFailure {
                            track_id: it.id,
                            file_name: it.old_name.clone(),
                            code: format!("rename_{}", it.status),
                            message: it.status.clone(),
                        });
                    }
                    continue;
                }
                let (vol, rel): (i64, String) = st.query_row([it.id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                let Some(mount) = mounts.get(&vol) else { continue };
                let to_rel = join_rel(&it.folder, it.new_name.as_deref().unwrap_or_default());
                moves.push(Move {
                    id: it.id,
                    volume_id: vol,
                    from_abs: volumes::join_rel(mount, &rel),
                    to_abs: volumes::join_rel(mount, &to_rel),
                    from_rel: rel,
                    to_rel,
                });
            }
        }
        if moves.is_empty() {
            return Ok(report);
        }
        let batch = history::begin_batch(&conn, "history.rename", None)?;
        report.batch_id = Some(batch);

        // Ordre d'exécution : d'abord les renommages dont la cible est libre.
        // Les échanges (A→B et B→A) passent par un nom temporaire.
        let mut pending: Vec<Move> = moves;
        let mut temp_of: HashMap<i64, (PathBuf, String)> = HashMap::new();
        while !pending.is_empty() {
            let free_idx = pending.iter().position(|m| {
                let same = m.from_abs.to_string_lossy().to_lowercase() == m.to_abs.to_string_lossy().to_lowercase();
                same || !m.to_abs.exists()
            });
            let Some(i) = free_idx else {
                // Cycle : le premier fichier prend un nom temporaire.
                let m = &mut pending[0];
                let ext = m.from_abs.extension().and_then(|e| e.to_str()).unwrap_or("tmp").to_string();
                let tmp_name = format!(".djlm-ren-{}.{ext}", m.id);
                let (dir, _) = split_rel(&m.from_rel);
                let tmp_rel = join_rel(dir, &tmp_name);
                let tmp_abs = m.from_abs.with_file_name(&tmp_name);
                match move_file(&conn, m.id, m.volume_id, &m.from_abs, &tmp_abs, &tmp_rel) {
                    Ok(()) => {
                        temp_of.insert(m.id, (m.from_abs.clone(), m.from_rel.clone()));
                        m.from_abs = tmp_abs;
                        m.from_rel = tmp_rel;
                    }
                    Err(e) => {
                        let m = pending.remove(0);
                        report.failures.push(EditFailure { track_id: m.id, file_name: m.from_rel.clone(), code: "rename_io".into(), message: e });
                    }
                }
                continue;
            };
            let m = pending.remove(i);
            let original = temp_of.get(&m.id).cloned().unwrap_or((m.from_abs.clone(), m.from_rel.clone()));
            match move_file(&conn, m.id, m.volume_id, &m.from_abs, &m.to_abs, &m.to_rel) {
                Ok(()) => {
                    let before = PathState { volume_id: m.volume_id, rel_path: original.1.clone() };
                    let after = PathState { volume_id: m.volume_id, rel_path: m.to_rel.clone() };
                    history::record(&conn, batch, "move", Some(m.id), &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
                    log::info!("Renommé : {} → {}", original.0.display(), m.to_abs.display());
                    report.ok += 1;
                }
                Err(e) => {
                    // Un fichier passé par un nom temporaire reprend son nom d'origine.
                    if temp_of.contains_key(&m.id) {
                        let _ = move_file(&conn, m.id, m.volume_id, &m.from_abs, &original.0, &original.1);
                    }
                    report.failures.push(EditFailure { track_id: m.id, file_name: m.from_rel.clone(), code: "rename_io".into(), message: e });
                }
            }
        }
        if history::drop_if_empty(&conn, batch)? {
            report.batch_id = None;
        }
        self.notify_changed(true);
        Ok(report)
    }

    /// Annulation d'un renommage ou d'un déplacement : le fichier retrouve son chemin enregistré.
    pub(crate) fn restore_path_state(&self, conn: &Connection, id: i64, state_json: &str) -> Result<(), EditFailure> {
        let fail = |code: &str, msg: String| EditFailure { track_id: id, file_name: String::new(), code: code.into(), message: msg };
        let state: PathState = serde_json::from_str(state_json).map_err(|e| fail("internal", e.to_string()))?;
        let (vol, rel): (i64, String) = conn
            .query_row("SELECT volume_id, rel_path FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| fail("internal", e.to_string()))?;
        if rel == state.rel_path && vol == state.volume_id {
            return Ok(());
        }
        let Some(mount) = self.mounts().get(&vol).cloned() else { return Err(fail("offline", "disque déconnecté".into())) };
        let from = volumes::join_rel(&mount, &rel);
        let to = volumes::join_rel(&mount, &state.rel_path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| fail("rename_io", e.to_string()))?;
        }
        move_file(conn, id, vol, &from, &to, &state.rel_path).map_err(|e| EditFailure {
            track_id: id,
            file_name: rel.rsplit('/').next().unwrap_or(&rel).to_string(),
            code: "rename_io".into(),
            message: e,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::Source;
    use crate::tags::write::Changes;

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn renommer_50_fichiers_puis_tout_annuler() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        for i in 0..50 {
            crate::testdata::write(&music, &format!("piste_{i:02}.wav"), &crate::testdata::wav_bytes(i));
        }
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        for (n, id) in ids.iter().enumerate() {
            let c = Changes { title: Some(Some(format!("Titre {n}"))), artists: Some(vec!["Artiste".into()]), ..Default::default() };
            lib.edit(&[*id], c, Source::Manual, "x", None).unwrap();
        }
        let before = names(&music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let preview = plan(&lib, &conn, &ids, template::DEFAULT_TEMPLATE, OnConflict::Suffix).unwrap();
        assert!(preview.iter().all(|p| p.status == "ok"), "{preview:?}");
        let r = lib.rename(&ids, template::DEFAULT_TEMPLATE, OnConflict::Suffix).unwrap();
        assert_eq!(r.ok, 50, "{:?}", r.failures);
        assert!(names(&music).contains(&"Artiste - Titre 7.wav".to_string()));
        let undo = lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert_eq!(undo.ok, 50, "{:?}", undo.failures);
        assert_eq!(names(&music), before, "les noms d'origine ne sont pas revenus");
        let db_name: String = conn.query_row("SELECT file_name FROM tracks WHERE id = ?1", [ids[7]], |r| r.get(0)).unwrap();
        assert!(db_name.starts_with("piste_"));
    }

    #[test]
    fn conflits_et_echanges() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        crate::testdata::write(&music, "a.mp3", &crate::testdata::mp3_bytes());
        crate::testdata::write(&music, "b.mp3", &crate::testdata::mp3_bytes());
        crate::testdata::write(&music, "c.mp3", &crate::testdata::mp3_bytes());
        crate::testdata::write(&music, "X - Pris.mp3", b"fichier tiers, pas dans la selection");
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let id_of = |name: &str| -> i64 { conn.query_row("SELECT id FROM tracks WHERE file_name = ?1", [name], |r| r.get(0)).unwrap() };
        let (a, b, c) = (id_of("a.mp3"), id_of("b.mp3"), id_of("c.mp3"));
        // a et b visent le même nom ; c vise un nom déjà pris par un fichier hors sélection.
        for (id, title) in [(a, "Même"), (b, "Même"), (c, "Pris")] {
            lib.edit(&[id], Changes { title: Some(Some(title.into())), artists: Some(vec!["X".into()]), ..Default::default() }, Source::Manual, "x", None).unwrap();
        }
        let skip = plan(&lib, &conn, &[a, b, c], "{artiste_principal} - {titre}", OnConflict::Skip).unwrap();
        assert_eq!(skip.iter().map(|p| p.status.as_str()).collect::<Vec<_>>(), ["ok", "conflict", "conflict"]);
        let r = lib.rename(&[a, b, c], "{artiste_principal} - {titre}", OnConflict::Suffix).unwrap();
        assert_eq!(r.ok, 3, "{:?}", r.failures);
        let n = names(&music);
        assert!(n.contains(&"X - Même.mp3".to_string()) && n.contains(&"X - Même (2).mp3".to_string()), "{n:?}");
        assert!(n.contains(&"X - Pris (2).mp3".to_string()) && n.contains(&"X - Pris.mp3".to_string()));
        assert_eq!(std::fs::read(music.join("X - Pris.mp3")).unwrap(), b"fichier tiers, pas dans la selection", "fichier tiers écrasé !");

        // Échange de noms : a ↔ b
        let name_a: String = conn.query_row("SELECT file_name FROM tracks WHERE id = ?1", [a], |r| r.get(0)).unwrap();
        lib.edit(&[a], Changes { title: Some(Some("Même (2)".into())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        lib.edit(&[b], Changes { title: Some(Some("Même".into())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let r = lib.rename(&[a, b], "{artiste_principal} - {titre}", OnConflict::Skip).unwrap();
        assert_eq!(r.ok, 2, "{:?}", r.failures);
        let new_a: String = conn.query_row("SELECT file_name FROM tracks WHERE id = ?1", [a], |r| r.get(0)).unwrap();
        assert_ne!(new_a, name_a);
        assert!(!names(&music).iter().any(|n| n.starts_with(".djlm")), "nom temporaire resté");
        let _ = ids;
    }

    #[test]
    fn nom_reserve_et_vide_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        crate::testdata::write(&music, "a.mp3", &crate::testdata::mp3_bytes());
        crate::testdata::write(&music, "b.mp3", &crate::testdata::mp3_bytes());
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        lib.edit(&ids[..1], Changes { title: Some(Some("CON".into())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let p = plan(&lib, &conn, &ids, "{titre}", OnConflict::Skip).unwrap();
        assert_eq!(p[0].status, "reserved");
        assert_eq!(p[1].status, "empty");
    }
}
