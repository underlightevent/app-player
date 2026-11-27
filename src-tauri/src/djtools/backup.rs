//! Sauvegarde de la bibliothèque et copie vers une clé USB.
//!
//! Chaque fichier est copié dans un fichier temporaire, comparé à l'original (empreinte
//! BLAKE3 de tout le fichier), puis renommé ; il reprend la date de modification de
//! l'original, ce qui permet de ne recopier ensuite que ce qui a changé. Rien n'est jamais
//! effacé dans la destination, et les fichiers de la bibliothèque ne sont jamais modifiés.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use crate::error::AppError;
use crate::library::{volumes, Library};
use crate::tasks::TaskHandle;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyReport {
    pub copied: usize,
    /// Déjà à jour dans la destination.
    pub skipped: usize,
    pub bytes: u64,
    pub failures: Vec<CopyFailure>,
    /// Morceaux sur un disque débranché.
    pub offline: usize,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyFailure {
    pub file: String,
    pub message: String,
}

fn file_hash(p: &Path) -> std::io::Result<blake3::Hash> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(p)?);
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize())
}

/// Même taille et même date de modification (à 2 s près : FAT et exFAT arrondissent).
fn up_to_date(src: &std::fs::Metadata, dst: &Path) -> bool {
    let Ok(d) = std::fs::metadata(dst) else { return false };
    if d.len() != src.len() {
        return false;
    }
    match (src.modified(), d.modified()) {
        (Ok(a), Ok(b)) => {
            let diff = if a > b { a.duration_since(b) } else { b.duration_since(a) };
            diff.map(|x| x.as_secs() <= 2).unwrap_or(false)
        }
        _ => false,
    }
}

/// Copie vérifiée de `src` vers `dst` (dossiers créés au besoin). Renvoie la taille copiée.
pub fn copy_verified(src: &Path, dst: &Path) -> Result<u64, String> {
    let meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = dst.with_file_name(format!(".djlm-copie-{}.tmp", std::process::id()));
    let res = (|| -> Result<u64, String> {
        let n = std::fs::copy(src, &tmp).map_err(|e| e.to_string())?;
        if file_hash(src).map_err(|e| e.to_string())? != file_hash(&tmp).map_err(|e| e.to_string())? {
            return Err("la copie n'est pas identique à l'original".into());
        }
        if let Ok(m) = meta.modified() {
            if let Ok(f) = File::options().write(true).open(&tmp) {
                let _ = f.set_modified(m);
            }
        }
        // Une ancienne copie différente n'est jamais écrasée : elle est renommée
        // « nom (ancienne AAAA-MM-JJ).ext », et la nouvelle prend sa place.
        if dst.exists() {
            let stem = dst.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = dst.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            let mut old = dst.with_file_name(format!("{stem} (ancienne {}){ext}", crate::db::today()));
            if old.exists() {
                old = free_name(&old);
            }
            std::fs::rename(dst, &old).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, dst).map_err(|e| e.to_string())?;
        Ok(n)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

/// « nom (2).ext », « nom (3).ext »… : premier nom libre.
pub fn free_name(p: &Path) -> PathBuf {
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 2..10_000 {
        let c = p.with_file_name(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    p.with_file_name(format!("{stem} ({}){ext}", std::process::id()))
}

/// Nom de dossier sûr sur tous les systèmes (nom d'un disque, d'une playlist).
pub fn safe_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 32 { '_' } else { c })
        .collect();
    out = out.trim().trim_end_matches('.').to_string();
    if out.is_empty() {
        "Sans nom".into()
    } else {
        out
    }
}

struct Src {
    id: i64,
    path: PathBuf,
    rel: String,
    label: String,
    file_name: String,
}

fn sources(lib: &Library, conn: &Connection, ids: Option<&[i64]>) -> Result<(Vec<Src>, usize), AppError> {
    let mounts = lib.mounts();
    let mut st = conn.prepare(
        "SELECT t.id, t.volume_id, t.rel_path, v.label, t.file_name FROM tracks t JOIN volumes v ON v.id = t.volume_id
         WHERE t.status NOT IN ('quarantined', 'missing')",
    )?;
    let all: Vec<(i64, i64, String, String, String)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    let mut offline = 0;
    let mut out = Vec::new();
    let order: Option<std::collections::HashMap<i64, usize>> = ids.map(|v| v.iter().enumerate().map(|(i, id)| (*id, i)).collect());
    for (id, vol, rel, label, file_name) in all {
        if let Some(o) = &order {
            if !o.contains_key(&id) {
                continue;
            }
        }
        match mounts.get(&vol) {
            Some(m) => out.push(Src { id, path: volumes::join_rel(m, &rel), rel, label, file_name }),
            None => offline += 1,
        }
    }
    if let Some(o) = &order {
        out.sort_by_key(|s| o.get(&s.id).copied().unwrap_or(usize::MAX));
    }
    Ok((out, offline))
}

fn run_copies(items: Vec<(PathBuf, PathBuf)>, task: Option<&TaskHandle>, report: &mut CopyReport) {
    if let Some(t) = task {
        t.set_total(items.len() as u64);
    }
    for (i, (src, dst)) in items.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            report.cancelled = true;
            break;
        }
        match std::fs::metadata(src) {
            Ok(m) if up_to_date(&m, dst) => report.skipped += 1,
            Ok(_) => match copy_verified(src, dst) {
                Ok(n) => {
                    report.copied += 1;
                    report.bytes += n;
                }
                Err(e) => report.failures.push(CopyFailure { file: src.to_string_lossy().into_owned(), message: e }),
            },
            Err(e) => report.failures.push(CopyFailure { file: src.to_string_lossy().into_owned(), message: e.to_string() }),
        }
        if let Some(t) = task {
            t.progress(i as u64 + 1);
        }
    }
}

/// La destination ne doit pas être à l'intérieur d'un dossier de la bibliothèque
/// (la sauvegarde serait elle-même indexée, puis sauvegardée…).
fn check_dest(lib: &Library, conn: &Connection, dest: &Path) -> Result<(), AppError> {
    let dest = std::fs::canonicalize(dest).map_err(|_| AppError::InvalidSetting("dossier de destination introuvable".into()))?;
    for root in crate::library::index::roots(conn)? {
        if let Some(m) = lib.mounts().get(&root.volume_id) {
            if let Ok(r) = std::fs::canonicalize(volumes::join_rel(m, &root.rel_path)) {
                if dest.starts_with(&r) {
                    return Err(AppError::InvalidSetting("la destination est dans un dossier de la bibliothèque".into()));
                }
            }
        }
    }
    Ok(())
}

/// Sauvegarde : `dest/<nom du disque>/<chemin d'origine>`, seulement ce qui a changé.
pub fn backup(lib: &Library, conn: &Connection, dest: &Path, task: Option<&TaskHandle>) -> Result<CopyReport, AppError> {
    check_dest(lib, conn, dest)?;
    let (src, offline) = sources(lib, conn, None)?;
    let items = src.into_iter().map(|s| {
        let d = volumes::join_rel(&dest.join(safe_name(&s.label)), &s.rel);
        (s.path, d)
    });
    let mut report = CopyReport { offline, ..Default::default() };
    run_copies(items.collect(), task, &mut report);
    Ok(report)
}

/// Rangement des fichiers copiés vers une clé.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// Tous les fichiers dans le dossier choisi.
    Flat,
    /// Un dossier au nom de la playlist / du set.
    Playlist,
    /// Arborescence d'origine.
    Folders,
}

/// Copie des morceaux `ids` vers `dest` (clé USB), avec une playlist M3U8 à chemins relatifs.
pub fn copy_to(lib: &Library, conn: &Connection, ids: &[i64], dest: &Path, layout: Layout, name: &str, m3u: bool, task: Option<&TaskHandle>) -> Result<CopyReport, AppError> {
    check_dest(lib, conn, dest)?;
    let (src, offline) = sources(lib, conn, Some(ids))?;
    let base = match layout {
        Layout::Playlist => dest.join(safe_name(name)),
        _ => dest.to_path_buf(),
    };
    let mut used: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut items = Vec::new();
    let mut entries = Vec::new();
    for s in src {
        let mut d = match layout {
            Layout::Folders => volumes::join_rel(&base, &s.rel),
            _ => base.join(&s.file_name),
        };
        // Deux fichiers de même nom à plat : le second est renommé « (2) ».
        if used.contains(&d) {
            let stem = d.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = d.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            let mut n = 2;
            while used.contains(&d) {
                d = d.with_file_name(format!("{stem} ({n}){ext}"));
                n += 1;
            }
        }
        used.insert(d.clone());
        entries.push((s.id, d.clone()));
        items.push((s.path, d));
    }
    let mut report = CopyReport { offline, ..Default::default() };
    run_copies(items, task, &mut report);
    if m3u && !report.cancelled {
        let list = base.join(format!("{}.m3u8", safe_name(name)));
        let mut text = String::from("#EXTM3U\n");
        for (id, d) in &entries {
            let rel = d.strip_prefix(&base).unwrap_or(d);
            let info: Option<(Option<String>, Option<String>, Option<i64>)> = conn
                .query_row("SELECT main_artist, title, duration_ms FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .ok();
            if let Some((a, t, ms)) = info {
                text.push_str(&format!("#EXTINF:{},{} - {}\n", ms.map(|m| m / 1000).unwrap_or(-1), a.unwrap_or_default(), t.unwrap_or_default()));
            }
            text.push_str(&volumes::to_rel_string(rel));
            text.push('\n');
        }
        std::fs::create_dir_all(&base)?;
        std::fs::write(&list, text)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::tests_support::library_on;
    use crate::testdata;

    #[test]
    fn sauvegarde_puis_seconde_passe_incrementale() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(music.join("House")).unwrap();
        let a = testdata::write(&music.join("House"), "a.wav", &testdata::wav_bytes(1));
        testdata::write(&music, "b.flac", &testdata::flac_bytes(2));
        let dest = dir.path().join("sauvegarde");
        std::fs::create_dir_all(&dest).unwrap();
        let (lib, _) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();

        let r = backup(&lib, &conn, &dest, None).unwrap();
        assert_eq!((r.copied, r.skipped, r.failures.len()), (2, 0, 0), "{:?}", r.failures);
        let label = conn.query_row("SELECT label FROM volumes", [], |r| r.get::<_, String>(0)).unwrap();
        let rel: String = conn.query_row("SELECT rel_path FROM tracks WHERE file_name = 'a.wav'", [], |r| r.get(0)).unwrap();
        let copy = volumes::join_rel(&dest.join(safe_name(&label)), &rel);
        assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&a).unwrap(), "copie identique");

        let r = backup(&lib, &conn, &dest, None).unwrap();
        assert_eq!((r.copied, r.skipped), (0, 2), "rien n'a changé");

        // Un fichier modifié est recopié ; l'ancienne copie n'est pas écrasée (gardée à côté).
        std::thread::sleep(std::time::Duration::from_millis(3100));
        std::fs::write(&a, testdata::wav_bytes(3)).unwrap();
        let r = backup(&lib, &conn, &dest, None).unwrap();
        assert_eq!((r.copied, r.skipped), (1, 1));
        assert_eq!(std::fs::read(&copy).unwrap(), testdata::wav_bytes(3), "nouvelle version à sa place");
        let old = copy.with_file_name(format!("a (ancienne {}).wav", crate::db::today()));
        assert_eq!(std::fs::read(&old).unwrap(), testdata::wav_bytes(1), "ancienne copie gardée");
        let r = backup(&lib, &conn, &dest, None).unwrap();
        assert_eq!((r.copied, r.skipped), (0, 2), "plus rien à recopier");

        // Destination à l'intérieur de la bibliothèque : refusée.
        assert!(backup(&lib, &conn, &music, None).is_err());
    }

    #[test]
    fn copie_vers_cle_avec_m3u() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(music.join("A")).unwrap();
        std::fs::create_dir_all(music.join("B")).unwrap();
        testdata::write(&music.join("A"), "x.wav", &testdata::wav_bytes(1));
        testdata::write(&music.join("B"), "x.wav", &testdata::wav_bytes(2));
        let key = dir.path().join("cle");
        std::fs::create_dir_all(&key).unwrap();
        let (lib, ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let r = copy_to(&lib, &conn, &ids, &key, Layout::Playlist, "Soirée : 27/11", true, None).unwrap();
        assert_eq!(r.copied, 2, "{:?}", r.failures);
        let folder = key.join("Soirée _ 27_11");
        assert!(folder.join("x.wav").exists() && folder.join("x (2).wav").exists(), "même nom : second renommé");
        let m3u = std::fs::read_to_string(folder.join("Soirée _ 27_11.m3u8")).unwrap();
        assert!(m3u.starts_with("#EXTM3U\n") && m3u.contains("\nx.wav\n") && m3u.contains("\nx (2).wav\n"), "{m3u}");
        assert!(music.join("A").join("x.wav").exists(), "originaux jamais déplacés");
    }
}
