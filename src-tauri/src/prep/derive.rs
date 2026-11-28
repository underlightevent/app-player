//! Nouveau fichier WAV fabriqué à partir d'un morceau (Remaster, DJ edit, instrumental,
//! acapella…). Garanties communes :
//! - l'original est seulement lu ;
//! - le nouveau fichier est écrit dans un fichier temporaire à côté, ses tags et sa pochette
//!   recopiés de l'original (version complétée), puis renommé sous un nom libre
//!   « … (Libellé).wav » (jamais d'écrasement) ;
//! - il est ajouté à la bibliothèque ; l'annulation l'envoie en quarantaine (jamais effacé).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

use crate::error::AppError;
use crate::library::quarantine::QuarantineState;
use crate::library::{index, volumes, Library};
use crate::tags::write::Changes;

#[derive(Debug, Clone)]
pub struct Derived {
    pub track_id: i64,
    pub file_name: String,
    pub path: PathBuf,
}

/// Nom libre « … (Libellé).wav » à côté de l'original.
fn target_rel(mount: &Path, rel: &str, label: &str) -> String {
    let (dir, file) = rel.rsplit_once('/').map(|(d, f)| (format!("{d}/"), f)).unwrap_or((String::new(), rel));
    let stem = Path::new(file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let base = format!("{dir}{stem} ({label}).wav");
    if !volumes::join_rel(mount, &base).exists() {
        return base;
    }
    for n in 2..1000 {
        let c = format!("{dir}{stem} ({label} {n}).wav");
        if !volumes::join_rel(mount, &c).exists() {
            return c;
        }
    }
    format!("{dir}{stem} ({label} {}).wav", std::process::id())
}

/// Version écrite dans le nouveau fichier : celle de l'original complétée de `word`.
pub fn derived_version(orig: Option<&str>, word: &str) -> String {
    match orig.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if v.to_lowercase().contains(&word.to_lowercase()) => v.to_string(),
        Some(v) => format!("{v} {word}"),
        None => word.to_string(),
    }
}

/// Fabrique « … (`label`).wav » à partir du morceau `id` : `render(original, temporaire)`
/// écrit le WAV. `reason` : motif de quarantaine en cas d'annulation.
pub fn derive<T>(
    lib: &Library,
    conn: &Connection,
    id: i64,
    label: &str,
    reason: &str,
    batch: i64,
    render: impl FnOnce(&Path, &Path) -> Result<T, String>,
) -> Result<(Derived, T), AppError> {
    let (vol, root_id, rel): (i64, Option<i64>, String) =
        conn.query_row("SELECT volume_id, root_id, rel_path FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    let mount = lib.mounts().get(&vol).cloned().ok_or(AppError::Offline)?;
    let src = volumes::join_rel(&mount, &rel);
    let new_rel = target_rel(&mount, &rel, label);
    let dst = volumes::join_rel(&mount, &new_rel);
    let tmp: PathBuf = dst.with_file_name(format!(".djlm-{reason}-{id}-{}.wav", std::process::id()));
    let io = |e: String| AppError::Io(std::io::Error::other(e));
    let cleanup = |e: AppError| {
        let _ = std::fs::remove_file(&tmp);
        e
    };
    let out = render(&src, &tmp).map_err(|e| cleanup(io(e)))?;

    // Tags et pochette de l'original ; version complétée.
    let exceptions = index::artist_exceptions(conn).unwrap_or_default();
    let ws = crate::library::edit::write_settings(conn);
    let orig = crate::tags::read::read_file(&src, &exceptions).map_err(|e| cleanup(io(e.to_string())))?;
    let now = crate::tags::read::read_file(&tmp, &exceptions).map_err(|e| cleanup(io(e.to_string())))?;
    let t = &orig.tags;
    let mut changes = Changes {
        title: Some(t.title.clone()),
        version: Some(Some(derived_version(t.version.as_deref(), label))),
        artists: Some(t.artists.clone()),
        album: Some(t.album.clone()),
        year: Some(t.year),
        genre: Some(t.genre.clone()),
        bpm: Some(t.bpm),
        key: Some(t.key.clone()),
        rating: Some(t.rating),
        ..Default::default()
    };
    if orig.cover.count > 0 {
        if let Ok(Some(c)) = crate::tags::write::current_cover(&src) {
            changes.cover = Some(Some(c));
        }
    }
    crate::tags::write::write_tags(&tmp, &now.tags, &changes, &ws, &exceptions).map_err(|e| cleanup(io(e.to_string())))?;
    if dst.exists() {
        return Err(cleanup(AppError::InvalidSetting("un fichier de ce nom existe déjà".into())));
    }
    std::fs::rename(&tmp, &dst).map_err(|e| cleanup(e.into()))?;
    log::info!("Nouveau fichier ({reason}) : {}", dst.display());

    let meta = std::fs::metadata(&dst)?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let new_id = index::store(conn, vol, root_id, &new_rel, meta.len() as i64, mtime, crate::tags::read::read_file(&dst, &exceptions))?;
    // Annulation : le nouveau fichier part en quarantaine (jamais effacé).
    let root_rel: Option<String> = conn.query_row("SELECT rel_path FROM roots WHERE id = ?1", [root_id], |r| r.get(0)).optional()?.flatten();
    let qrel = match root_rel.as_deref().filter(|r| !r.is_empty() && new_rel.starts_with(&format!("{r}/"))) {
        Some(r) => format!("{r}/{}/{}-{reason}/{}", index::QUARANTINE_DIR, crate::db::today(), &new_rel[r.len() + 1..]),
        None => format!("{}/{}-{reason}/{new_rel}", index::QUARANTINE_DIR, crate::db::today()),
    };
    let before = QuarantineState { volume_id: vol, rel_path: qrel, quarantined: true, reason: Some(reason.into()) };
    let after = QuarantineState { volume_id: vol, rel_path: new_rel.clone(), quarantined: false, reason: None };
    crate::history::record(conn, batch, "quarantine", Some(new_id), &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
    Ok((
        Derived { track_id: new_id, file_name: dst.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), path: dst },
        out,
    ))
}
