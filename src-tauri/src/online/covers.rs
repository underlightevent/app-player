//! Téléchargement des pochettes candidates (Cover Art Archive, Deezer, iTunes).
//! Les pochettes téléchargées sont gardées sur l'ordinateur pour ne pas les retélécharger,
//! sauf celles de Deezer (copie locale interdite par ses conditions).

use std::path::{Path, PathBuf};

use super::http::{Http, NetError};

fn cache_file(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{}.img", blake3::hash(url.as_bytes()).to_hex()))
}

pub fn fetch(http: &Http, cache_dir: &Path, url: &str, source: &str) -> Result<Vec<u8>, NetError> {
    let keep = source != "deezer";
    let file = cache_file(cache_dir, url);
    if keep {
        if let Ok(b) = std::fs::read(&file) {
            return Ok(b);
        }
    }
    let http_source = if source == "coverartarchive" { "coverartarchive" } else { source };
    // Pas de cache JSON pour les images : elles vont dans le dossier dédié.
    let bytes = http.get(http_source, url, &[("Accept", "image/*")], false)?;
    if keep {
        let _ = std::fs::create_dir_all(cache_dir);
        let _ = std::fs::write(&file, &bytes);
    }
    Ok(bytes)
}

/// Dimensions d'une image (sans la décoder entièrement).
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}
