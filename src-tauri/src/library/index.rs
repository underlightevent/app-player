//! Indexation : parcours des dossiers, lecture des fichiers, enregistrement en base.
//! Ces fonctions n'ont pas besoin de Tauri : elles sont testées directement.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};

use super::volumes::{self, MountedVolume};
use crate::error::AppError;
use crate::tags::read::{read_file, ReadError};
use crate::tags::text::nfc;
use crate::tags::{is_audio_path, FileInfo};
use crate::tasks::TaskHandle;

/// Dossier de quarantaine, jamais indexé.
pub const QUARANTINE_DIR: &str = "_Quarantaine";

#[derive(Debug, Clone)]
pub struct RootRow {
    pub id: i64,
    pub volume_id: i64,
    pub rel_path: String,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ScanStats {
    pub seen: u64,
    pub read: u64,
    pub unreadable: u64,
    pub missing: u64,
}

pub fn upsert_volume(conn: &Connection, v: &MountedVolume) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO volumes (uid, label, last_mount) VALUES (?1, ?2, ?3)
         ON CONFLICT(uid) DO UPDATE SET label = excluded.label, last_mount = excluded.last_mount",
        params![v.uid, v.label, v.mount.to_string_lossy()],
    )?;
    Ok(conn.query_row("SELECT id FROM volumes WHERE uid = ?1", [&v.uid], |r| r.get(0))?)
}

pub fn add_root(conn: &Connection, volume_id: i64, rel_path: &str, kind: &str) -> Result<i64, AppError> {
    conn.execute(
        "INSERT INTO roots (volume_id, rel_path, kind) VALUES (?1, ?2, ?3)
         ON CONFLICT(volume_id, rel_path) DO UPDATE SET kind = excluded.kind",
        params![volume_id, rel_path, kind],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM roots WHERE volume_id = ?1 AND rel_path = ?2",
        params![volume_id, rel_path],
        |r| r.get(0),
    )?)
}

pub fn roots(conn: &Connection) -> Result<Vec<RootRow>, AppError> {
    let mut stmt = conn.prepare("SELECT id, volume_id, rel_path FROM roots ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| Ok(RootRow { id: r.get(0)?, volume_id: r.get(1)?, rel_path: r.get(2)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn artist_exceptions(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT name FROM artist_exceptions ORDER BY length(name) DESC")?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

fn mtime_of(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Fichiers à ignorer : cachés, fichiers « ._ » de macOS sur disques exFAT.
fn skip_name(name: &str) -> bool {
    name.starts_with('.') || name.starts_with("._") || name == QUARANTINE_DIR
}

pub struct Found {
    pub rel: String,
    pub abs: PathBuf,
    pub size: i64,
    pub mtime: i64,
}

/// Liste les fichiers audio sous `dir`. Renvoie aussi `complete = false` si une erreur
/// d'accès a empêché de tout voir (dans ce cas on ne marque rien comme manquant).
pub fn walk(mount: &Path, dir: &Path, cancel: Option<&TaskHandle>) -> (Vec<Found>, bool) {
    let mut out = Vec::new();
    let mut complete = true;
    let walker = walkdir::WalkDir::new(dir).follow_links(false).into_iter().filter_entry(|e| {
        e.depth() == 0 || !e.file_name().to_str().map(skip_name).unwrap_or(false)
    });
    for entry in walker {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            return (out, false);
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Parcours : {e}");
                complete = false;
                continue;
            }
        };
        if !entry.file_type().is_file() || !is_audio_path(entry.path()) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            complete = false;
            continue;
        };
        let rel = volumes::to_rel_string(entry.path().strip_prefix(mount).unwrap_or(entry.path()));
        out.push(Found { rel, abs: entry.path().to_path_buf(), size: meta.len() as i64, mtime: mtime_of(&meta) });
    }
    (out, complete)
}

struct Existing {
    id: i64,
    size: i64,
    mtime: i64,
    status: String,
}

/// Indexe une racine : seuls les fichiers nouveaux ou modifiés (taille ou date) sont relus.
pub fn scan_root(
    conn: &mut Connection,
    root: &RootRow,
    mount: &Path,
    exceptions: &[String],
    task: Option<&TaskHandle>,
    on_batch: &dyn Fn(),
) -> Result<ScanStats, AppError> {
    let dir = volumes::join_rel(mount, &root.rel_path);
    if !dir.is_dir() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("dossier introuvable : {}", dir.display()),
        )));
    }
    let (found, complete) = walk(mount, &dir, task);
    let mut stats = ScanStats { seen: found.len() as u64, ..Default::default() };

    let mut existing: HashMap<String, Existing> = HashMap::new();
    {
        let prefix = if root.rel_path.is_empty() { String::new() } else { format!("{}/", root.rel_path) };
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, size, mtime, status FROM tracks
             WHERE volume_id = ?1 AND (root_id = ?2 OR substr(rel_path, 1, length(?3)) = ?3)",
        )?;
        let rows = stmt.query_map(params![root.volume_id, root.id, prefix], |r| {
            Ok((r.get::<_, String>(1)?, Existing { id: r.get(0)?, size: r.get(2)?, mtime: r.get(3)?, status: r.get(4)? }))
        })?;
        for row in rows {
            let (rel, e) = row?;
            existing.insert(rel, e);
        }
    }

    let mut to_read: Vec<&Found> = Vec::new();
    let mut seen_ids: Vec<i64> = Vec::new();
    for f in &found {
        match existing.get(&f.rel) {
            // Un fichier illisible est relu à chaque scan : une nouvelle version de l'application
            // sait peut-être le lire.
            Some(e) if e.size == f.size && e.mtime == f.mtime && e.status != "missing" && e.status != "unreadable" => {
                seen_ids.push(e.id)
            }
            Some(e) => {
                seen_ids.push(e.id);
                to_read.push(f);
            }
            None => to_read.push(f),
        }
    }
    if let Some(t) = task {
        t.set_total(to_read.len() as u64);
    }

    // Lecture en parallèle par lots, écriture en base par transaction.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    let mut done = 0u64;
    for batch in to_read.chunks(64) {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        let results: Vec<(&Found, Result<FileInfo, ReadError>)> =
            pool.install(|| batch.par_iter().map(|f| (*f, read_file(&f.abs, exceptions))).collect());
        let tx = conn.transaction()?;
        for (f, res) in results {
            if res.is_err() {
                stats.unreadable += 1;
            }
            store(&tx, root.volume_id, Some(root.id), &f.rel, f.size, f.mtime, res)?;
            stats.read += 1;
        }
        tx.commit()?;
        done += batch.len() as u64;
        if let Some(t) = task {
            t.progress(done);
        }
        on_batch();
    }

    // Fichiers disparus : marqués « manquants » (jamais effacés de la base).
    if complete && !task.map(|t| t.is_cancelled()).unwrap_or(false) {
        let seen: std::collections::HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        let tx = conn.transaction()?;
        for (rel, e) in &existing {
            if !seen.contains(rel.as_str()) && e.status != "missing" && e.status != "quarantined" {
                tx.execute("UPDATE tracks SET status = 'missing' WHERE id = ?1", [e.id])?;
                stats.missing += 1;
            }
        }
        tx.commit()?;
        if stats.missing > 0 {
            on_batch();
        }
    }
    Ok(stats)
}

/// Champs dont la valeur en base prime si l'utilisateur les a verrouillés.
fn locked_fields(conn: &Connection, track_id: i64) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT field FROM field_state WHERE track_id = ?1 AND locked = 1")?;
    let rows = stmt.query_map([track_id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

/// Enregistre le résultat de lecture d'un fichier.
pub fn store(
    conn: &Connection,
    volume_id: i64,
    root_id: Option<i64>,
    rel: &str,
    size: i64,
    mtime: i64,
    res: Result<FileInfo, ReadError>,
) -> Result<i64, AppError> {
    let file_name = nfc(rel.rsplit('/').next().unwrap_or(rel));
    let format = Path::new(&file_name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let previous_row: Option<(i64, i64, String)> = conn
        .query_row(
            "SELECT size, mtime, status FROM tracks WHERE volume_id = ?1 AND rel_path = ?2",
            params![volume_id, rel],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let was_unreadable = previous_row.as_ref().map(|p| p.2 == "unreadable").unwrap_or(false);
    let previous: Option<(i64, i64)> = previous_row.map(|p| (p.0, p.1));
    let file_changed = previous.map(|p| p != (size, mtime)).unwrap_or(true);
    conn.execute(
        "INSERT INTO tracks (volume_id, rel_path, file_name, root_id, size, mtime, format)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(volume_id, rel_path) DO UPDATE SET
            root_id = COALESCE(excluded.root_id, tracks.root_id), size = excluded.size, mtime = excluded.mtime",
        params![volume_id, rel, file_name, root_id, size, mtime, format],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM tracks WHERE volume_id = ?1 AND rel_path = ?2",
        params![volume_id, rel],
        |r| r.get(0),
    )?;
    // Nouveau fichier dans un dossier d'arrivée : il entre dans l'Inbox (phase 2).
    // Un fichier illisible à son arrivée y entre dès qu'il devient lisible.
    if (previous.is_none() || was_unreadable) && res.is_ok() {
        if let Some(rid) = root_id {
            let kind: Option<String> = conn.query_row("SELECT kind FROM roots WHERE id = ?1", [rid], |r| r.get(0)).optional()?;
            if kind.as_deref() == Some("inbox") {
                crate::online::register_inbox(conn, id)?;
            }
        }
    }

    let info = match res {
        Ok(info) => info,
        Err(e) => {
            conn.execute(
                "UPDATE tracks SET status = 'unreadable', error = ?2, indexed_at = unixepoch() WHERE id = ?1",
                params![id, e.to_string()],
            )?;
            return Ok(id);
        }
    };

    let locked = locked_fields(conn, id)?;
    let keep = |f: &str| locked.iter().any(|l| l == f);
    let t = &info.tags;
    let main_artist = t.artists.first().cloned();
    let artists_text = if t.artists.is_empty() { None } else { Some(t.artists.join(", ")) };
    conn.execute(
        "UPDATE tracks SET
            format = ?2, codec = ?3, bitrate = ?4, sample_rate = ?5, bit_depth = ?6, channels = ?7,
            duration_ms = ?8, cover_count = ?9, cover_width = ?10, cover_height = ?11, wav_desync = ?12,
            status = 'ok', error = NULL, indexed_at = unixepoch(),
            audio_hash = CASE WHEN ?13 THEN NULL ELSE audio_hash END
         WHERE id = ?1",
        params![
            id,
            info.format,
            info.codec,
            info.bitrate,
            info.sample_rate,
            info.bit_depth,
            info.channels,
            info.duration_ms as i64,
            info.cover.count,
            info.cover.width,
            info.cover.height,
            info.wav_desync,
            // Le hash est recalculé en tâche de fond quand le fichier a changé.
            file_changed
        ],
    )?;

    let set_text = |field: &str, column: &str, value: Option<&str>| -> Result<(), AppError> {
        if !keep(field) {
            conn.execute(&format!("UPDATE tracks SET {column} = ?2 WHERE id = ?1"), params![id, value])?;
        }
        Ok(())
    };
    set_text("title", "title", t.title.as_deref())?;
    set_text("version", "version", t.version.as_deref())?;
    set_text("album", "album", t.album.as_deref())?;
    set_text("genre", "genre", t.genre.as_deref())?;
    set_text("key", "musical_key", t.key.as_deref())?;
    set_text("remixer", "remixer", t.remixer.as_deref())?;
    set_text("label", "label", t.label.as_deref())?;
    if !keep("year") {
        conn.execute("UPDATE tracks SET year = ?2 WHERE id = ?1", params![id, t.year])?;
    }
    if !keep("bpm") {
        conn.execute("UPDATE tracks SET bpm = ?2 WHERE id = ?1", params![id, t.bpm])?;
    }
    if !keep("rating") {
        conn.execute("UPDATE tracks SET rating = ?2 WHERE id = ?1", params![id, t.rating])?;
    }
    if !keep("artists") {
        conn.execute(
            "UPDATE tracks SET artists = ?2, main_artist = ?3 WHERE id = ?1",
            params![id, artists_text, main_artist],
        )?;
        conn.execute("DELETE FROM track_artists WHERE track_id = ?1", [id])?;
        for (i, a) in t.artists.iter().enumerate() {
            conn.execute("INSERT INTO track_artists (track_id, position, name) VALUES (?1, ?2, ?3)", params![id, i as i64, a])?;
        }
    }
    Ok(id)
}

/// Morceaux d'un volume en ligne dont le hash audio reste à calculer.
pub fn pending_hashes(conn: &Connection, volume_ids: &[i64], limit: usize) -> Result<Vec<(i64, i64, String)>, AppError> {
    if volume_ids.is_empty() {
        return Ok(vec![]);
    }
    let list = volume_ids.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
    let mut stmt = conn.prepare(&format!(
        "SELECT id, volume_id, rel_path FROM tracks
         WHERE audio_hash IS NULL AND status = 'ok' AND volume_id IN ({list}) LIMIT {limit}"
    ))?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_hash(conn: &Connection, id: i64, hash: Option<&str>, error: Option<&str>) -> Result<(), AppError> {
    match (hash, error) {
        (Some(h), _) => conn.execute("UPDATE tracks SET audio_hash = ?2 WHERE id = ?1", params![id, h])?,
        (None, Some(e)) => conn.execute(
            "UPDATE tracks SET status = 'unreadable', error = ?2 WHERE id = ?1",
            params![id, e],
        )?,
        _ => 0,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn indexation_complete_puis_incrementale() {
        use crate::testdata;
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("Musique");
        std::fs::create_dir_all(music.join("Techno")).unwrap();
        std::fs::create_dir_all(music.join(QUARANTINE_DIR)).unwrap();
        testdata::all_formats(&music.join("Techno"));
        testdata::write(&music, "casse.mp3", b"pas du tout un mp3");
        testdata::write(&music, "._cache.wav", b"metadonnees macOS");
        testdata::write(&music.join(QUARANTINE_DIR), "ecarte.wav", &testdata::wav_bytes(9));
        testdata::write(&music, "notes.txt", b"texte");

        let mut conn = open_in_memory().unwrap();
        let (vol, rel) = volumes::locate(&music).unwrap();
        let vol_id = upsert_volume(&conn, &vol).unwrap();
        let root_id = add_root(&conn, vol_id, &rel, "library").unwrap();
        let root = RootRow { id: root_id, volume_id: vol_id, rel_path: rel.clone() };

        let stats = scan_root(&mut conn, &root, &vol.mount, &[], None, &|| {}).unwrap();
        assert_eq!(stats.seen, 5, "{stats:?}"); // 4 formats + le fichier cassé
        assert_eq!(stats.unreadable, 1);
        let unreadable: i64 =
            conn.query_row("SELECT count(*) FROM tracks WHERE status = 'unreadable'", [], |r| r.get(0)).unwrap();
        assert_eq!(unreadable, 1);

        // Deuxième passage : rien n'a changé, seul le fichier illisible est relu (une nouvelle
        // version de l'application sait peut-être le lire).
        let stats = scan_root(&mut conn, &root, &vol.mount, &[], None, &|| {}).unwrap();
        assert_eq!(stats.read, 1);
        assert_eq!(stats.unreadable, 1);

        // Un fichier supprimé est marqué manquant, pas effacé.
        std::fs::remove_file(music.join("Techno").join("test.mp3")).unwrap();
        let stats = scan_root(&mut conn, &root, &vol.mount, &[], None, &|| {}).unwrap();
        assert_eq!(stats.missing, 1);
        let missing: i64 =
            conn.query_row("SELECT count(*) FROM tracks WHERE status = 'missing'", [], |r| r.get(0)).unwrap();
        assert_eq!(missing, 1);

        // Il revient : il redevient normal.
        testdata::write(&music.join("Techno"), "test.mp3", &testdata::mp3_bytes());
        scan_root(&mut conn, &root, &vol.mount, &[], None, &|| {}).unwrap();
        let missing: i64 =
            conn.query_row("SELECT count(*) FROM tracks WHERE status = 'missing'", [], |r| r.get(0)).unwrap();
        assert_eq!(missing, 0);
    }

    #[test]
    fn noms_ignores() {
        assert!(skip_name("._Titre.wav"));
        assert!(skip_name(".DS_Store"));
        assert!(skip_name(QUARANTINE_DIR));
        assert!(!skip_name("Titre.wav"));
    }

    #[test]
    fn fichier_illisible_marque_et_garde() {
        let conn = open_in_memory().unwrap();
        let vol = upsert_volume(
            &conn,
            &MountedVolume { uid: "test".into(), label: "TEST".into(), mount: PathBuf::from("/") },
        )
        .unwrap();
        let id = store(&conn, vol, None, "a/b.mp3", 10, 1, Err(ReadError::Parse("cassé".into()))).unwrap();
        let (status, name): (String, String) =
            conn.query_row("SELECT status, file_name FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(status, "unreadable");
        assert_eq!(name, "b.mp3");
    }

    #[test]
    fn champ_verrouille_non_ecrase_par_relecture() {
        let conn = open_in_memory().unwrap();
        let vol = upsert_volume(
            &conn,
            &MountedVolume { uid: "test".into(), label: "TEST".into(), mount: PathBuf::from("/") },
        )
        .unwrap();
        let mut info = FileInfo::default();
        info.tags.title = Some("Du fichier".into());
        let id = store(&conn, vol, None, "x.mp3", 1, 1, Ok(info.clone())).unwrap();
        conn.execute("UPDATE tracks SET title = 'Saisi à la main' WHERE id = ?1", [id]).unwrap();
        conn.execute("INSERT INTO field_state (track_id, field, locked, source) VALUES (?1, 'title', 1, 'manual')", [id]).unwrap();
        store(&conn, vol, None, "x.mp3", 2, 2, Ok(info)).unwrap();
        let title: String = conn.query_row("SELECT title FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(title, "Saisi à la main");
    }
}
