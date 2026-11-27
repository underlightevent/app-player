//! Rekordbox : uniquement par le format XML officiel (jamais la base interne).
//!
//! - Import de la collection exportée par Rekordbox (« Export Collection in xml format ») :
//!   les morceaux retrouvés portent le badge « Rekordbox ».
//! - Mode protégé : pas de renommage ni de déplacement automatique d'un morceau badgé,
//!   alerte avant un renommage ou un déplacement manuel.
//! - Liste « À recharger » (tags modifiés depuis l'import) et export M3U8.
//! - Journal des chemins (ancien ; nouveau), exportable en CSV.
//! - Export XML vers Rekordbox (panneau Bridge, rubrique « rekordbox xml »).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::AppError;
use crate::library::{volumes, Library};
use crate::tags::text::nfc;

/// Au-delà, l'application propose de réimporter le XML.
pub const STALE_DAYS: i64 = 7;

// ---------------- Adresses « file://localhost/… » ----------------

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Adresse Rekordbox → chemin (« file://localhost/E:/Musique/A%20B.wav » → « E:/Musique/A B.wav »).
pub fn decode_location(loc: &str) -> Option<String> {
    let rest = loc.strip_prefix("file://localhost").or_else(|| loc.strip_prefix("file://"))?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // « /E:/… » (Windows) → « E:/… »
    let b = s.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return Some(s[1..].to_string());
    }
    Some(s)
}

/// Chemin → adresse Rekordbox (caractères spéciaux et accents encodés).
pub fn encode_location(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://localhost");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric() || "-._~/:".contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Chemin relatif au disque d'après un chemin complet de Rekordbox :
/// « E:/Musique/x.wav » → « Musique/x.wav » ; « /Volumes/MUSIQUE/Musique/x.wav » → « Musique/x.wav » ;
/// « /Users/moi/Musique/x.wav » → « Users/moi/Musique/x.wav ».
fn rel_of(path: &str) -> (Option<String>, String) {
    let b = path.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/' {
        return (None, path[3..].to_string());
    }
    if let Some(rest) = path.strip_prefix("/Volumes/") {
        if let Some((label, rel)) = rest.split_once('/') {
            return (Some(label.to_string()), rel.to_string());
        }
    }
    (None, path.trim_start_matches('/').to_string())
}

fn key(s: &str) -> String {
    nfc(s).to_lowercase()
}

// ---------------- Import du XML ----------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub xml_tracks: usize,
    pub matched: usize,
    /// Quelques exemples de morceaux de Rekordbox absents de la bibliothèque.
    pub unmatched_examples: Vec<String>,
}

/// Lit les pistes (identifiant, adresse) de la collection d'un XML Rekordbox.
pub fn parse_collection(xml: &str) -> Result<Vec<(String, String)>, AppError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    let mut in_collection = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                if name.as_ref() == "COLLECTION" {
                    in_collection = true;
                } else if in_collection && name.as_ref() == "TRACK" {
                    let mut id = String::new();
                    let mut loc = String::new();
                    for a in e.attributes().flatten() {
                        let v = a
                            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map(|c| c.into_owned())
                            .unwrap_or_else(|_| a.value.to_string());
                        match a.key.as_ref() {
                            "TrackID" => id = v,
                            "Location" => loc = v,
                            _ => {}
                        }
                    }
                    if !loc.is_empty() {
                        out.push((id, loc));
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == "COLLECTION" => in_collection = false,
            Ok(Event::Eof) => break,
            Err(e) => return Err(AppError::InvalidSetting(format!("XML illisible : {e}"))),
            _ => {}
        }
    }
    Ok(out)
}

/// Importe la collection Rekordbox : remplace les badges précédents.
pub fn import(lib: &Library, file: &Path) -> Result<ImportReport, AppError> {
    let xml = std::fs::read_to_string(file)?;
    let entries = parse_collection(&xml)?;
    if entries.is_empty() && !xml.contains("DJ_PLAYLISTS") {
        return Err(AppError::InvalidSetting("ce fichier n'est pas un export XML de Rekordbox".into()));
    }
    let mut conn = crate::db::open_worker(lib.db_path())?;
    let mounts = lib.mounts();
    // Index des morceaux par chemin relatif, et par chemin complet quand le disque est branché.
    let mut by_rel: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    let mut by_abs: HashMap<String, i64> = HashMap::new();
    let labels: HashMap<i64, String> = {
        let mut st = conn.prepare("SELECT id, label FROM volumes")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<HashMap<_, _>, _>>()?;
        v
    };
    {
        let mut st = conn.prepare("SELECT id, volume_id, rel_path FROM tracks WHERE status != 'quarantined'")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (id, vol, rel) = row?;
            if let Some(m) = mounts.get(&vol) {
                by_abs.insert(key(&volumes::join_rel(m, &rel).to_string_lossy().replace('\\', "/")), id);
            }
            by_rel.entry(key(&rel)).or_default().push((id, vol));
        }
    }
    let mut matched: Vec<(i64, String, String)> = Vec::new();
    let mut unmatched = Vec::new();
    for (rb_id, loc) in &entries {
        let Some(path) = decode_location(loc) else { continue };
        let found = by_abs.get(&key(&path)).copied().or_else(|| {
            let (label, rel) = rel_of(&path);
            let list = by_rel.get(&key(&rel))?;
            let pick: Vec<&(i64, i64)> = match &label {
                Some(l) => list.iter().filter(|(_, v)| labels.get(v).map(|x| x.eq_ignore_ascii_case(l)).unwrap_or(false)).collect(),
                None => list.iter().collect(),
            };
            // Un seul candidat possible : sinon on ne devine pas.
            if pick.len() == 1 {
                Some(pick[0].0)
            } else {
                None
            }
        });
        match found {
            Some(id) => matched.push((id, rb_id.clone(), loc.clone())),
            None => {
                if unmatched.len() < 10 {
                    unmatched.push(path);
                }
            }
        }
    }
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM rekordbox_tracks", [])?;
    for (id, rb, loc) in &matched {
        tx.execute("INSERT OR REPLACE INTO rekordbox_tracks (track_id, rb_id, location) VALUES (?1, ?2, ?3)", params![id, rb, loc])?;
    }
    // Les morceaux à recharger qui ne sont plus dans Rekordbox sortent de la liste.
    tx.execute("DELETE FROM rekordbox_reload WHERE track_id NOT IN (SELECT track_id FROM rekordbox_tracks)", [])?;
    tx.execute(
        "INSERT OR REPLACE INTO rekordbox_import (id, file, imported_at, xml_tracks, matched) VALUES (1, ?1, unixepoch(), ?2, ?3)",
        params![file.to_string_lossy(), entries.len() as i64, matched.len() as i64],
    )?;
    tx.commit()?;
    log::info!("Rekordbox : {} morceaux dans le XML, {} retrouvés", entries.len(), matched.len());
    lib.notify_changed(true);
    Ok(ImportReport { xml_tracks: entries.len(), matched: matched.len(), unmatched_examples: unmatched })
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub file: Option<String>,
    pub imported_at: Option<i64>,
    pub xml_tracks: i64,
    pub matched: i64,
    /// Import de plus de 7 jours : proposer de réimporter.
    pub stale: bool,
    pub reload_count: i64,
    pub journal_count: i64,
    pub protected: bool,
}

pub fn protected(conn: &Connection) -> bool {
    crate::settings::get(conn, "rekordbox_protected").ok().flatten().map(|v| v != "false").unwrap_or(true)
}

pub fn status(conn: &Connection) -> Result<Status, AppError> {
    let mut s = conn
        .query_row("SELECT file, imported_at, xml_tracks, matched FROM rekordbox_import WHERE id = 1", [], |r| {
            Ok(Status { file: r.get(0)?, imported_at: r.get(1)?, xml_tracks: r.get(2)?, matched: r.get(3)?, ..Default::default() })
        })
        .optional()?
        .unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    s.stale = s.imported_at.map(|t| now - t > STALE_DAYS * 86_400).unwrap_or(false);
    s.reload_count = conn.query_row("SELECT count(*) FROM rekordbox_reload", [], |r| r.get(0))?;
    s.journal_count = conn.query_row("SELECT count(*) FROM path_journal", [], |r| r.get(0))?;
    s.protected = protected(conn);
    Ok(s)
}

pub fn in_rekordbox(conn: &Connection, id: i64) -> bool {
    conn.query_row("SELECT 1 FROM rekordbox_tracks WHERE track_id = ?1", [id], |_| Ok(())).optional().ok().flatten().is_some()
}

/// Après une modification de tags : un morceau badgé entre dans « À recharger ».
pub fn mark_reload(conn: &Connection, id: i64) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO rekordbox_reload (track_id, changed_at) SELECT ?1, unixepoch() WHERE EXISTS (SELECT 1 FROM rekordbox_tracks WHERE track_id = ?1)
         ON CONFLICT(track_id) DO UPDATE SET changed_at = excluded.changed_at",
        [id],
    )?;
    Ok(())
}

pub fn reload_ids(conn: &Connection) -> Result<Vec<i64>, AppError> {
    let mut st = conn.prepare("SELECT r.track_id FROM rekordbox_reload r JOIN tracks t ON t.id = r.track_id ORDER BY r.changed_at")?;
    let v = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
    Ok(v)
}

pub fn clear_reload(conn: &Connection, ids: Option<&[i64]>) -> Result<(), AppError> {
    match ids {
        None => conn.execute("DELETE FROM rekordbox_reload", [])?,
        Some(list) => {
            for id in list {
                conn.execute("DELETE FROM rekordbox_reload WHERE track_id = ?1", [id])?;
            }
            0
        }
    };
    Ok(())
}

// ---------------- Journal des chemins ----------------

/// Inscrit un renommage ou un déplacement (pour relocaliser les fichiers dans Rekordbox).
pub fn journal(conn: &Connection, track_id: i64, old_path: &Path, new_path: &Path) {
    let res = conn.execute(
        "INSERT INTO path_journal (track_id, old_path, new_path, in_rekordbox)
         VALUES (?1, ?2, ?3, EXISTS (SELECT 1 FROM rekordbox_tracks WHERE track_id = ?1))",
        params![track_id, old_path.to_string_lossy(), new_path.to_string_lossy()],
    );
    if let Err(e) = res {
        log::warn!("Journal des chemins : {e}");
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Journal au format CSV (séparateur « ; », lisible par Excel en français), avec BOM UTF-8.
pub fn journal_csv(conn: &Connection, only_rekordbox: bool) -> Result<String, AppError> {
    let mut st = conn.prepare(
        "SELECT old_path, new_path, in_rekordbox, at FROM path_journal WHERE (?1 = 0 OR in_rekordbox = 1) ORDER BY id",
    )?;
    let mut out = String::from("\u{FEFF}ancien chemin;nouveau chemin;dans Rekordbox;date\r\n");
    let rows = st.query_map([only_rekordbox], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?, r.get::<_, i64>(3)?)))?;
    for row in rows {
        let (old, new, rb, at) = row?;
        out.push_str(&format!(
            "{};{};{};{}\r\n",
            csv_field(&old),
            csv_field(&new),
            if rb { "oui" } else { "non" },
            date_of(at)
        ));
    }
    Ok(out)
}

/// Date et heure (temps universel) lisibles, sans dépendance supplémentaire.
fn date_of(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

// ---------------- Exports ----------------

struct ExportRow {
    id: i64,
    path: Option<PathBuf>,
    title: Option<String>,
    version: Option<String>,
    artists: Vec<String>,
    album: Option<String>,
    genre: Option<String>,
    year: Option<i64>,
    bpm: Option<f64>,
    key: Option<String>,
    rating: i64,
    remixer: Option<String>,
    label: Option<String>,
    format: String,
    size: i64,
    duration_ms: Option<i64>,
    bitrate: Option<i64>,
    sample_rate: Option<i64>,
}

fn export_rows(conn: &Connection, mounts: &HashMap<i64, PathBuf>, ids: &[i64]) -> Result<Vec<ExportRow>, AppError> {
    let mut st = conn.prepare(
        "SELECT id, volume_id, rel_path, title, version, album, genre, year, bpm, musical_key, rating, remixer, label, format,
                size, duration_ms, bitrate, sample_rate FROM tracks WHERE id = ?1 AND status != 'quarantined'",
    )?;
    let mut ast = conn.prepare("SELECT name FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
    let mut out = Vec::new();
    for id in ids {
        let row = st
            .query_row([id], |r| {
                let vol: i64 = r.get(1)?;
                let rel: String = r.get(2)?;
                Ok(ExportRow {
                    id: r.get(0)?,
                    path: mounts.get(&vol).map(|m| volumes::join_rel(m, &rel)),
                    title: r.get(3)?,
                    version: r.get(4)?,
                    artists: vec![],
                    album: r.get(5)?,
                    genre: r.get(6)?,
                    year: r.get(7)?,
                    bpm: r.get(8)?,
                    key: r.get(9)?,
                    rating: r.get(10)?,
                    remixer: r.get(11)?,
                    label: r.get(12)?,
                    format: r.get(13)?,
                    size: r.get(14)?,
                    duration_ms: r.get(15)?,
                    bitrate: r.get(16)?,
                    sample_rate: r.get(17)?,
                })
            })
            .optional()?;
        if let Some(mut row) = row {
            row.artists = ast.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
            out.push(row);
        }
    }
    Ok(out)
}

/// Playlist M3U8 (UTF-8) : chemins complets, avec durée, artiste et titre.
pub fn m3u(conn: &Connection, mounts: &HashMap<i64, PathBuf>, ids: &[i64]) -> Result<(String, usize), AppError> {
    let rows = export_rows(conn, mounts, ids)?;
    let mut out = String::from("#EXTM3U\n");
    let mut skipped = 0;
    for r in rows {
        let Some(p) = r.path else {
            skipped += 1;
            continue;
        };
        let title = crate::tags::text::compose_title(r.title.as_deref().unwrap_or(""), r.version.as_deref(), true);
        let secs = r.duration_ms.map(|d| d / 1000).unwrap_or(-1);
        out.push_str(&format!("#EXTINF:{secs},{} - {title}\n{}\n", r.artists.join(", "), p.to_string_lossy()));
    }
    Ok((out, skipped))
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => o.push(c),
        }
    }
    o
}

/// Note de 0 à 5 étoiles → échelle Rekordbox.
pub fn rb_rating(stars: i64) -> i64 {
    [0, 51, 102, 153, 204, 255][stars.clamp(0, 5) as usize]
}

fn kind_of(format: &str) -> &'static str {
    match format {
        "wav" => "WAV File",
        "aif" | "aiff" => "AIFF File",
        "flac" => "FLAC File",
        "m4a" => "M4A File",
        "ogg" => "OGG File",
        _ => "MP3 File",
    }
}

#[derive(Debug, Clone)]
pub struct ExportPlaylist {
    pub name: String,
    pub ids: Vec<i64>,
}

/// Fichier XML au format Rekordbox : collection et playlists (les filtres intelligents
/// sont exportés comme playlists, avec leurs morceaux du moment).
pub fn export_xml(
    conn: &Connection,
    mounts: &HashMap<i64, PathBuf>,
    ids: &[i64],
    playlists: &[ExportPlaylist],
    version_in_title: bool,
    artist_in_title: bool,
) -> Result<(String, usize, usize), AppError> {
    let mut all: Vec<i64> = ids.to_vec();
    for p in playlists {
        for id in &p.ids {
            if !all.contains(id) {
                all.push(*id);
            }
        }
    }
    let rows = export_rows(conn, mounts, &all)?;
    let mut exported: Vec<i64> = Vec::new();
    let mut skipped = 0;
    let mut col = String::new();
    for r in &rows {
        let Some(p) = &r.path else {
            skipped += 1;
            continue;
        };
        exported.push(r.id);
        // Même titre que celui écrit dans le fichier (réglages « version » et « artiste dans le titre »).
        let name = crate::tags::text::full_title(
            r.title.as_deref().unwrap_or(""),
            r.version.as_deref(),
            r.artists.first().map(String::as_str),
            version_in_title,
            artist_in_title,
        );
        col.push_str(&format!(
            "    <TRACK TrackID=\"{}\" Name=\"{}\" Artist=\"{}\" Composer=\"\" Album=\"{}\" Grouping=\"\" Genre=\"{}\" Kind=\"{}\" Size=\"{}\" TotalTime=\"{}\" DiscNumber=\"0\" TrackNumber=\"0\" Year=\"{}\" AverageBpm=\"{}\" DateAdded=\"{}\" BitRate=\"{}\" SampleRate=\"{}\" Comments=\"\" PlayCount=\"0\" Rating=\"{}\" Location=\"{}\" Remixer=\"{}\" Tonality=\"{}\" Label=\"{}\" Mix=\"{}\"/>\n",
            r.id,
            esc(&name),
            esc(&r.artists.join(", ")),
            esc(r.album.as_deref().unwrap_or("")),
            esc(r.genre.as_deref().unwrap_or("")),
            kind_of(&r.format),
            r.size,
            r.duration_ms.map(|d| d / 1000).unwrap_or(0),
            r.year.map(|y| y.to_string()).unwrap_or_default(),
            r.bpm.map(|b| format!("{b:.2}")).unwrap_or_else(|| "0.00".into()),
            crate::db::today(),
            r.bitrate.unwrap_or(0),
            r.sample_rate.unwrap_or(0),
            rb_rating(r.rating),
            esc(&encode_location(p)),
            esc(r.remixer.as_deref().unwrap_or("")),
            esc(r.key.as_deref().unwrap_or("")),
            esc(r.label.as_deref().unwrap_or("")),
            esc(r.version.as_deref().unwrap_or("")),
        ));
    }
    let mut pl = String::new();
    for p in playlists {
        let members: Vec<&i64> = p.ids.iter().filter(|id| exported.contains(id)).collect();
        pl.push_str(&format!("        <NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">\n", esc(&p.name), members.len()));
        for id in members {
            pl.push_str(&format!("          <TRACK Key=\"{id}\"/>\n"));
        }
        pl.push_str("        </NODE>\n");
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\">\n  <PRODUCT Name=\"dj-library-manager\" Version=\"{}\" Company=\"\"/>\n  <COLLECTION Entries=\"{}\">\n{col}  </COLLECTION>\n  <PLAYLISTS>\n    <NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\n      <NODE Type=\"0\" Name=\"DJ Library Manager\" Count=\"{}\">\n{pl}      </NODE>\n    </NODE>\n  </PLAYLISTS>\n</DJ_PLAYLISTS>\n",
        env!("CARGO_PKG_VERSION"),
        exported.len(),
        playlists.len()
    );
    Ok((xml, exported.len(), skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::tests_support::library_on;
    use crate::testdata;

    #[test]
    fn adresses_aller_retour() {
        let p = Path::new("E:/Musique/Été & Co/A B#1.wav");
        let loc = encode_location(p);
        assert_eq!(loc, "file://localhost/E:/Musique/%C3%89t%C3%A9%20%26%20Co/A%20B%231.wav");
        assert_eq!(decode_location(&loc).unwrap(), "E:/Musique/Été & Co/A B#1.wav");
        let mac = encode_location(Path::new("/Volumes/MUSIQUE/x.mp3"));
        assert_eq!(mac, "file://localhost/Volumes/MUSIQUE/x.mp3");
        assert_eq!(rel_of(&decode_location(&mac).unwrap()), (Some("MUSIQUE".into()), "x.mp3".into()));
        assert_eq!(rel_of("E:/Musique/x.mp3"), (None, "Musique/x.mp3".into()));
        assert_eq!(rb_rating(4), 204);
    }

    fn xml_with(locations: &[String]) -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<DJ_PLAYLISTS Version=\"1.0.0\"><PRODUCT Name=\"rekordbox\"/>\n<COLLECTION Entries=\"2\">\n");
        for (i, l) in locations.iter().enumerate() {
            s.push_str(&format!("<TRACK TrackID=\"{}\" Name=\"x\" Location=\"{}\"><TEMPO Inizio=\"0\"/></TRACK>\n", 100 + i, esc(l)));
        }
        s.push_str("</COLLECTION><PLAYLISTS><NODE Type=\"0\" Name=\"ROOT\" Count=\"0\"></NODE></PLAYLISTS></DJ_PLAYLISTS>");
        s
    }

    #[test]
    fn import_badges_rechargement_et_journal() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(music.join("Sous dossier é")).unwrap();
        let a = testdata::write(&music.join("Sous dossier é"), "Été 1.wav", &testdata::wav_bytes(1));
        testdata::write(&music, "b.flac", &testdata::flac_bytes(2));
        let (lib, _) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let id_a: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'Été 1.wav'", [], |r| r.get(0)).unwrap();
        let canon = std::fs::canonicalize(&a).unwrap();
        let canon = canon.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/");
        let xml = xml_with(&[encode_location(Path::new(&canon)), "file://localhost/Z:/ailleurs/inconnu.mp3".into()]);
        let file = dir.path().join("rekordbox.xml");
        std::fs::write(&file, xml).unwrap();
        let r = import(&lib, &file).unwrap();
        assert_eq!(r.xml_tracks, 2);
        assert_eq!(r.matched, 1, "{:?}", r.unmatched_examples);
        assert!(in_rekordbox(&conn, id_a));
        let s = status(&conn).unwrap();
        assert!(!s.stale);
        assert!(s.protected);

        // Une modification de tags met le morceau dans « À recharger ».
        lib.edit(&[id_a], crate::tags::write::Changes { title: Some(Some("Nouveau".into())), ..Default::default() }, crate::library::edit::Source::Manual, "history.editTags", None)
            .unwrap();
        assert_eq!(reload_ids(&conn).unwrap(), vec![id_a]);
        let (m3u, skipped) = m3u(&conn, &lib.mounts(), &reload_ids(&conn).unwrap()).unwrap();
        assert_eq!(skipped, 0);
        assert!(m3u.starts_with("#EXTM3U\n#EXTINF:0,"), "{m3u}");
        assert!(m3u.contains("Été 1.wav"));
        clear_reload(&conn, None).unwrap();
        assert!(reload_ids(&conn).unwrap().is_empty());

        // Renommage : inscrit au journal, avec la mention Rekordbox.
        let rep = lib.rename(&[id_a], "{titre}", crate::rename::OnConflict::Suffix).unwrap();
        assert_eq!(rep.ok, 1, "{:?}", rep.failures);
        let csv = journal_csv(&conn, false).unwrap();
        assert!(csv.starts_with("\u{FEFF}ancien chemin;nouveau chemin"));
        assert!(csv.contains("Été 1.wav;") && csv.contains("Nouveau.wav;oui;"), "{csv}");
    }

    #[test]
    fn export_xml_lisible_et_conforme() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        testdata::write(&music, "a & b.wav", &testdata::wav_bytes(1));
        testdata::write(&music, "c.flac", &testdata::flac_bytes(2));
        let (lib, ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        conn.execute("UPDATE tracks SET rating = 4, title = 'Titre \"x\"', version = 'Extended Mix', musical_key = 'Am'", []).unwrap();
        let pls = vec![ExportPlaylist { name: "Soirée <1>".into(), ids: vec![ids[1]] }];
        let (xml, n, skipped) = export_xml(&conn, &lib.mounts(), &[ids[0]], &pls, true, false).unwrap();
        assert_eq!((n, skipped), (2, 0));
        assert!(xml.contains("Rating=\"204\""));
        assert!(xml.contains("Mix=\"Extended Mix\""));
        assert!(xml.contains("Tonality=\"Am\""));
        assert!(xml.contains("Name=\"Titre &quot;x&quot; (Extended Mix)\""));
        assert!(xml.contains("Name=\"Soirée &lt;1&gt;\""));
        assert!(xml.contains("a%20%26%20b.wav"));
        // Relu par notre propre import : les deux morceaux sont retrouvés.
        let back = parse_collection(&xml).unwrap();
        assert_eq!(back.len(), 2);
        let file = dir.path().join("export.xml");
        std::fs::write(&file, &xml).unwrap();
        assert_eq!(import(&lib, &file).unwrap().matched, 2);
    }
}
