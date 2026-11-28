//! Dossier de téléchargements surveillé (phase 8) : règle créée par l'utilisateur,
//! désactivée par défaut (principe 9). Toutes les 20 s, les fichiers audio complets du
//! dossier choisi (taille stable depuis au moins 15 s) sont copiés dans l'Inbox ; la copie est
//! vérifiée octet pour octet, puis l'original part dans la corbeille du système
//! (récupérable). L'Inbox fait ensuite son travail habituel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OptionalExtension};

use crate::library::{volumes, Library};

/// Dossier de l'Inbox (racine de type « inbox »), s'il est branché.
fn inbox_dir(lib: &Library, conn: &Connection) -> Option<PathBuf> {
    let (vol, rel): (i64, String) = conn.query_row("SELECT volume_id, rel_path FROM roots WHERE kind = 'inbox' ORDER BY id LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional().ok()??;
    lib.mounts().get(&vol).map(|m| volumes::join_rel(m, &rel))
}

/// Fichiers prêts : audio, pas cachés, taille inchangée depuis le passage précédent et pas
/// modifiés depuis 15 s (téléchargement terminé).
fn ready(dir: &Path, seen: &mut HashMap<PathBuf, u64>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    let now = SystemTime::now();
    let mut current = HashMap::new();
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !p.is_file() || !crate::tags::is_audio_path(&p) {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        let old_enough = m.modified().ok().and_then(|t| now.duration_since(t).ok()).map(|d| d >= Duration::from_secs(15)).unwrap_or(false);
        if seen.get(&p) == Some(&m.len()) && old_enough && m.len() > 0 {
            out.push(p.clone());
        }
        current.insert(p, m.len());
    }
    *seen = current;
    out
}

/// Déplace un fichier vers l'Inbox : copie vérifiée, puis original à la corbeille.
pub fn move_to_inbox(src: &Path, inbox: &Path) -> Result<PathBuf, String> {
    move_with(src, inbox, |p| trash::delete(p).map_err(|e| e.to_string()))
}

/// Copie vérifiée, puis on se débarrasse de l'original (corbeille en vrai ; effacement dans les essais).
fn move_with(src: &Path, inbox: &Path, dispose: impl Fn(&Path) -> Result<(), String>) -> Result<PathBuf, String> {
    let name = src.file_name().ok_or("nom de fichier")?;
    let mut dst = inbox.join(name);
    if dst.exists() {
        dst = crate::djtools::backup::free_name(&dst);
    }
    crate::djtools::backup::copy_verified(src, &dst)?;
    dispose(src).map_err(|e| format!("copié, mais l'original n'a pas pu aller à la corbeille : {e}"))?;
    Ok(dst)
}

/// Surveillance en tâche de fond (vérifie le réglage à chaque passage).
pub fn spawn(lib: Arc<Library>) {
    std::thread::Builder::new()
        .name("telechargements".into())
        .spawn(move || {
            let mut seen: HashMap<PathBuf, u64> = HashMap::new();
            loop {
                std::thread::sleep(Duration::from_secs(20));
                let Ok(conn) = crate::db::open_worker(lib.db_path()) else { continue };
                let Ok(all) = crate::settings::get_all(&conn) else { continue };
                if all.get("downloads_watch").map(String::as_str) != Some("true") {
                    seen.clear();
                    continue;
                }
                let dir = all.get("downloads_dir").cloned().unwrap_or_default();
                if dir.is_empty() {
                    continue;
                }
                let Some(inbox) = inbox_dir(&lib, &conn) else { continue };
                let dir = PathBuf::from(dir);
                if dir == inbox || inbox.starts_with(&dir) {
                    continue; // jamais l'Inbox elle-même (ni un dossier qui la contient)
                }
                let files = ready(&dir, &mut seen);
                for f in files {
                    match move_to_inbox(&f, &inbox) {
                        Ok(d) => log::info!("Téléchargement rangé dans l'Inbox : {}", d.display()),
                        Err(e) => log::warn!("Téléchargement {} : {e}", f.display()),
                    }
                    seen.remove(&f);
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deplacement_verifie_sans_perte() {
        let dir = tempfile::tempdir().unwrap();
        let dl = dir.path().join("Téléchargements");
        let inbox = dir.path().join("Inbox");
        std::fs::create_dir_all(&dl).unwrap();
        std::fs::create_dir_all(&inbox).unwrap();
        let bytes = crate::testdata::wav_bytes(4);
        let src = crate::testdata::write(&dl, "Nouveau.wav", &bytes);
        std::fs::write(inbox.join("Nouveau.wav"), b"autre").unwrap();
        let dst = move_with(&src, &inbox, |p| std::fs::remove_file(p).map_err(|e| e.to_string())).unwrap();
        assert!(!src.exists());
        assert_ne!(dst, inbox.join("Nouveau.wav"), "pas d'écrasement");
        assert_eq!(std::fs::read(&dst).unwrap(), bytes);
        assert_eq!(std::fs::read(inbox.join("Nouveau.wav")).unwrap(), b"autre");
        // Fichier « en cours » : pas encore prêt au premier passage.
        let mut seen = HashMap::new();
        crate::testdata::write(&dl, "b.wav", &bytes);
        assert!(ready(&dl, &mut seen).is_empty());
    }
}
