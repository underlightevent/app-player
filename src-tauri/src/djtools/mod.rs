//! Outils du DJ (phase 6) : étiquettes personnelles, statistiques, préparation de sets,
//! historique de jeu. Les modules `backup`, `loudness`, `analysis` et `dates` touchent aux
//! fichiers ; celui-ci ne travaille que dans la base de l'application.

pub mod analysis;
pub mod backup;
pub mod dates;
pub mod loudness;

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::tags::text::to_camelot;

// =====================================================================
// Étiquettes personnelles (ambiance, événement…)
// =====================================================================

pub const TAG_COLORS: &[&str] = &["gray", "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTag {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub count: i64,
}

pub fn tags_list(conn: &Connection) -> Result<Vec<UserTag>, AppError> {
    let mut st = conn.prepare(
        "SELECT g.id, g.name, g.color,
                (SELECT count(*) FROM track_user_tags tt JOIN tracks t ON t.id = tt.track_id
                 WHERE tt.tag_id = g.id AND t.status != 'quarantined')
         FROM user_tags g ORDER BY g.name COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| Ok(UserTag { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, count: r.get(3)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn clean_name(name: &str) -> Result<String, AppError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 60 {
        return Err(AppError::InvalidSetting("nom d'étiquette vide ou trop long".into()));
    }
    Ok(n.to_string())
}

fn color(c: &str) -> &str {
    if TAG_COLORS.contains(&c) {
        c
    } else {
        "gray"
    }
}

/// Crée une étiquette (ou renvoie celle qui porte déjà ce nom).
pub fn tag_create(conn: &Connection, name: &str, col: &str) -> Result<i64, AppError> {
    let name = clean_name(name)?;
    if let Some(id) = conn.query_row("SELECT id FROM user_tags WHERE name = ?1", [&name], |r| r.get(0)).optional()? {
        return Ok(id);
    }
    conn.execute("INSERT INTO user_tags (name, color) VALUES (?1, ?2)", params![name, color(col)])?;
    Ok(conn.last_insert_rowid())
}

pub fn tag_update(conn: &Connection, id: i64, name: &str, col: &str) -> Result<(), AppError> {
    let name = clean_name(name)?;
    conn.execute("UPDATE user_tags SET name = ?2, color = ?3 WHERE id = ?1", params![id, name, color(col)])
        .map_err(|_| AppError::InvalidSetting("une étiquette porte déjà ce nom".into()))?;
    Ok(())
}

/// Supprime l'étiquette (les morceaux et les fichiers ne sont pas touchés).
pub fn tag_delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM user_tags WHERE id = ?1", [id])?;
    Ok(())
}

/// Pose (`on`) ou retire l'étiquette sur les morceaux. Renvoie le nombre de morceaux changés.
pub fn tag_apply(conn: &Connection, tag: i64, ids: &[i64], on: bool) -> Result<usize, AppError> {
    let mut n = 0;
    for id in ids {
        n += if on {
            conn.execute("INSERT OR IGNORE INTO track_user_tags (track_id, tag_id) VALUES (?1, ?2)", params![id, tag])?
        } else {
            conn.execute("DELETE FROM track_user_tags WHERE track_id = ?1 AND tag_id = ?2", params![id, tag])?
        };
    }
    Ok(n)
}

// =====================================================================
// Statistiques
// =====================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Count {
    pub label: String,
    pub count: i64,
    /// Taille totale en octets (formats, disques).
    pub size: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub tracks: i64,
    pub size: i64,
    pub duration_ms: i64,
    pub genres: Vec<Count>,
    pub years: Vec<Count>,
    pub formats: Vec<Count>,
    pub quality: Vec<Count>,
    pub disks: Vec<Count>,
    pub added_by_month: Vec<Count>,
    pub most_played: Vec<Count>,
    pub keys: Vec<Count>,
    pub bpm_ranges: Vec<Count>,
}

fn counts(conn: &Connection, sql: &str) -> Result<Vec<Count>, AppError> {
    let mut st = conn.prepare(sql)?;
    let rows = st.query_map([], |r| Ok(Count { label: r.get::<_, Option<String>>(0)?.unwrap_or_default(), count: r.get(1)?, size: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

const LIVE: &str = "status NOT IN ('quarantined', 'missing')";

pub fn stats(conn: &Connection) -> Result<Stats, AppError> {
    let (tracks, size, duration_ms): (i64, i64, i64) = conn.query_row(
        &format!("SELECT count(*), coalesce(sum(size), 0), coalesce(sum(duration_ms), 0) FROM tracks WHERE {LIVE}"),
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut keys = counts(conn, &format!("SELECT musical_key, count(*), 0 FROM tracks WHERE {LIVE} AND musical_key IS NOT NULL GROUP BY musical_key"))?;
    for k in keys.iter_mut() {
        if let Some(c) = to_camelot(&k.label) {
            k.label = c;
        }
    }
    keys.sort_by_key(|k| {
        let n: u32 = k.label.trim_end_matches(['A', 'B']).parse().unwrap_or(99);
        (n, k.label.ends_with('B'))
    });
    Ok(Stats {
        tracks,
        size,
        duration_ms,
        genres: counts(
            conn,
            &format!("SELECT genre, count(*), sum(size) FROM tracks WHERE {LIVE} AND coalesce(genre, '') != '' GROUP BY genre COLLATE NOCASE ORDER BY count(*) DESC LIMIT 25"),
        )?,
        years: counts(conn, &format!("SELECT CAST((year / 10) * 10 AS TEXT), count(*), sum(size) FROM tracks WHERE {LIVE} AND year IS NOT NULL GROUP BY year / 10 ORDER BY year / 10"))?,
        formats: counts(conn, &format!("SELECT upper(format), count(*), sum(size) FROM tracks WHERE {LIVE} GROUP BY format ORDER BY count(*) DESC"))?,
        quality: counts(
            conn,
            &format!(
                "SELECT CASE WHEN format IN ('wav', 'aiff', 'aif', 'flac') THEN 'lossless'
                             WHEN bitrate >= 320 THEN '320' WHEN bitrate >= 256 THEN '256'
                             WHEN bitrate >= 192 THEN '192' ELSE 'low' END AS q, count(*), sum(size)
                 FROM tracks WHERE {LIVE} GROUP BY q ORDER BY count(*) DESC"
            ),
        )?,
        disks: counts(conn, &format!("SELECT v.label, count(*), sum(t.size) FROM tracks t JOIN volumes v ON v.id = t.volume_id WHERE t.{LIVE} GROUP BY v.id ORDER BY sum(t.size) DESC"))?,
        added_by_month: counts(
            conn,
            &format!("SELECT strftime('%Y-%m', indexed_at, 'unixepoch') AS m, count(*), sum(size) FROM tracks WHERE {LIVE} AND indexed_at IS NOT NULL GROUP BY m ORDER BY m DESC LIMIT 24"),
        )?,
        most_played: counts(
            conn,
            "SELECT coalesce(t.main_artist || ' - ', '') || coalesce(t.title, t.file_name), count(*), 0
             FROM plays p JOIN tracks t ON t.id = p.track_id GROUP BY p.track_id ORDER BY count(*) DESC LIMIT 20",
        )?,
        keys,
        bpm_ranges: counts(
            conn,
            &format!("SELECT CAST((CAST(bpm AS INTEGER) / 10) * 10 AS TEXT), count(*), 0 FROM tracks WHERE {LIVE} AND bpm > 0 GROUP BY CAST(bpm AS INTEGER) / 10 ORDER BY CAST(bpm AS INTEGER) / 10"),
        )?,
    })
}

// =====================================================================
// Préparation de sets : compatibilité des enchaînements et suggestions
// =====================================================================

/// Position Camelot (1–12, A ou B) d'une tonalité « Am », « C#m », « 8A »…
fn camelot(key: &str) -> Option<(u8, char)> {
    let c = to_camelot(key).or_else(|| crate::tags::text::normalize_key(key).and_then(|k| to_camelot(&k)))?;
    let letter = c.chars().last()?;
    Some((c[..c.len() - 1].parse().ok()?, letter))
}

/// Compatibilité harmonique (roue Camelot) : « same » (même tonalité), « adjacent » (±1),
/// « relative » (même chiffre, A ↔ B), « energy » (+2, montée d'énergie), « clash » sinon.
pub fn key_compat(a: &str, b: &str) -> &'static str {
    let (Some((na, la)), Some((nb, lb))) = (camelot(a), camelot(b)) else { return "unknown" };
    let diff = ((nb as i32 - na as i32) + 12) % 12;
    match (diff, la == lb) {
        (0, true) => "same",
        (0, false) => "relative",
        (1, true) | (11, true) => "adjacent",
        (2, true) => "energy",
        _ => "clash",
    }
}

/// Écart de BPM en % (en tenant compte du demi / double tempo).
pub fn bpm_diff(a: f64, b: f64) -> Option<f64> {
    if a <= 0.0 || b <= 0.0 {
        return None;
    }
    [b, b * 2.0, b / 2.0].iter().map(|x| (x - a) / a * 100.0).min_by(|x, y| x.abs().total_cmp(&y.abs()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTrack {
    pub id: i64,
    pub title: Option<String>,
    pub version: Option<String>,
    pub artist: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub camelot: Option<String>,
    pub genre: Option<String>,
    pub duration_ms: Option<i64>,
    pub energy: Option<i64>,
    /// Avec le morceau précédent du set.
    pub key_compat: Option<String>,
    pub bpm_diff: Option<f64>,
}

fn track_info(conn: &Connection, id: i64) -> Result<Option<SetTrack>, AppError> {
    Ok(conn
        .query_row(
            "SELECT t.id, t.title, t.version, t.main_artist, t.bpm, t.musical_key, t.genre, t.duration_ms,
                    (SELECT energy FROM analysis a WHERE a.track_id = t.id)
             FROM tracks t WHERE t.id = ?1",
            [id],
            |r| {
                let key: Option<String> = r.get(5)?;
                Ok(SetTrack {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    version: r.get(2)?,
                    artist: r.get(3)?,
                    bpm: r.get(4)?,
                    camelot: key.as_deref().and_then(to_camelot),
                    key,
                    genre: r.get(6)?,
                    duration_ms: r.get(7)?,
                    energy: r.get(8)?,
                    key_compat: None,
                    bpm_diff: None,
                })
            },
        )
        .optional()?)
}

/// Morceaux d'un set dans l'ordre, avec la compatibilité de chaque enchaînement.
pub fn set_tracks(conn: &Connection, playlist: i64) -> Result<Vec<SetTrack>, AppError> {
    let ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?
        .query_map([playlist], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out: Vec<SetTrack> = Vec::new();
    for id in ids {
        let Some(mut t) = track_info(conn, id)? else { continue };
        if let Some(prev) = out.last() {
            if let (Some(a), Some(b)) = (&prev.key, &t.key) {
                t.key_compat = Some(key_compat(a, b).to_string());
            }
            if let (Some(a), Some(b)) = (prev.bpm, t.bpm) {
                t.bpm_diff = bpm_diff(a, b);
            }
        }
        out.push(t);
    }
    Ok(out)
}

/// Déplace un morceau du set (nouvelle position, à partir de 0).
pub fn set_move(conn: &Connection, playlist: i64, track: i64, to: usize) -> Result<(), AppError> {
    let mut ids: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?
        .query_map([playlist], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let Some(from) = ids.iter().position(|x| *x == track) else { return Ok(()) };
    let t = ids.remove(from);
    ids.insert(to.min(ids.len()), t);
    for (i, id) in ids.iter().enumerate() {
        conn.execute("UPDATE playlist_tracks SET position = ?3 WHERE playlist_id = ?1 AND track_id = ?2", params![playlist, id, i as i64])?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    #[serde(flatten)]
    pub track: SetTrack,
    pub score: i32,
}

/// Morceaux de la bibliothèque qui s'enchaînent bien après `after` : tonalité compatible,
/// BPM à ±`bpm_range` %, même genre en priorité ; morceaux déjà dans le set exclus.
pub fn suggestions(conn: &Connection, after: i64, exclude: &[i64], bpm_range: f64, limit: usize) -> Result<Vec<Suggestion>, AppError> {
    let Some(base) = track_info(conn, after)? else { return Ok(vec![]) };
    let (Some(bpm), Some(key)) = (base.bpm, base.key.clone()) else { return Ok(vec![]) };
    let mut st = conn.prepare(
        "SELECT id FROM tracks WHERE status NOT IN ('quarantined', 'missing', 'unreadable') AND bpm > 0 AND musical_key IS NOT NULL",
    )?;
    let ids: Vec<i64> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for id in ids {
        if id == after || exclude.contains(&id) {
            continue;
        }
        let Some(mut t) = track_info(conn, id)? else { continue };
        let (Some(b), Some(k)) = (t.bpm, t.key.clone()) else { continue };
        let Some(d) = bpm_diff(bpm, b) else { continue };
        if d.abs() > bpm_range {
            continue;
        }
        let compat = key_compat(&key, &k);
        let mut score = match compat {
            "same" => 40,
            "adjacent" | "relative" => 30,
            "energy" => 20,
            _ => continue,
        };
        score += (20.0 - d.abs() * 3.0).max(0.0) as i32;
        if let (Some(g1), Some(g2)) = (&base.genre, &t.genre) {
            if g1.eq_ignore_ascii_case(g2) {
                score += 15;
            }
        }
        t.key_compat = Some(compat.to_string());
        t.bpm_diff = Some(d);
        out.push(Suggestion { track: t, score });
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.track.id.cmp(&b.track.id)));
    out.truncate(limit);
    Ok(out)
}

// =====================================================================
// Historique de jeu (Rekordbox)
// =====================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryReport {
    /// Listes d'historique trouvées (une par soirée).
    pub sessions: usize,
    /// Passages enregistrés (nouveaux).
    pub plays: usize,
    /// Morceaux de l'historique introuvables dans la bibliothèque.
    pub unmatched: usize,
    pub unmatched_examples: Vec<String>,
}

/// Date « AAAA-MM-JJ » trouvée dans le nom d'une liste d'historique (« HISTORY 2026-11-27 »,
/// « Historique 2026-11-27 (2) », « 27/11/2026 »…).
pub fn date_in(name: &str) -> Option<String> {
    let digits: Vec<&str> = name.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).collect();
    for w in digits.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        if a.len() == 4 && b.len() <= 2 && c.len() <= 2 {
            return Some(format!("{a}-{:0>2}-{:0>2}", b, c));
        }
        if c.len() == 4 && a.len() <= 2 && b.len() <= 2 {
            return Some(format!("{c}-{:0>2}-{:0>2}", b, a));
        }
    }
    None
}

/// Enregistre des passages (morceau, date). Renvoie le nombre de nouveaux passages.
pub fn record_plays(conn: &Connection, plays: &[(i64, String)], source: &str) -> Result<usize, AppError> {
    let mut n = 0;
    for (id, date) in plays {
        n += conn.execute("INSERT OR IGNORE INTO plays (track_id, played_at, source) VALUES (?1, ?2, ?3)", params![id, date, source])?;
    }
    Ok(n)
}

/// Rapproche des chemins de fichiers (historique) des morceaux de la bibliothèque.
pub fn match_paths(conn: &Connection, mounts: &HashMap<i64, std::path::PathBuf>, paths: &[std::path::PathBuf]) -> Result<Vec<Option<i64>>, AppError> {
    let mut by_name: HashMap<String, Vec<(i64, i64, String)>> = HashMap::new();
    let mut st = conn.prepare("SELECT id, volume_id, rel_path, file_name FROM tracks WHERE status != 'quarantined'")?;
    for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))? {
        let (id, vol, rel, name) = row?;
        by_name.entry(name.to_lowercase()).or_default().push((id, vol, rel));
    }
    Ok(paths
        .iter()
        .map(|p| {
            let name = p.file_name()?.to_string_lossy().to_lowercase();
            let cands = by_name.get(&name)?;
            // Chemin complet identique d'abord, sinon un seul fichier de ce nom.
            let exact = cands.iter().find(|(_, vol, rel)| {
                mounts.get(vol).map(|m| crate::library::volumes::join_rel(m, rel).to_string_lossy().to_lowercase() == p.to_string_lossy().to_lowercase()).unwrap_or(false)
            });
            match (exact, cands.len()) {
                (Some((id, _, _)), _) => Some(*id),
                (None, 1) => Some(cands[0].0),
                _ => None,
            }
        })
        .collect())
}

/// Listes d'historique d'un XML Rekordbox : (date, chemins des fichiers). Une liste compte si
/// elle est dans un dossier « HISTORY » / « Historique » ou si son nom contient une date.
pub fn parse_xml_history(xml: &str) -> Result<Vec<(String, Vec<std::path::PathBuf>)>, AppError> {
    use quick_xml::events::Event;
    let locations: HashMap<String, String> = crate::rekordbox::parse_collection(xml)?.into_iter().collect();
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut current: Option<(String, Vec<std::path::PathBuf>)> = None;
    let mut out = Vec::new();
    let attr = |e: &quick_xml::events::BytesStart, key: &str| -> Option<String> {
        e.attributes().flatten().find(|a| a.key.as_ref() == key).map(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map(|c| c.into_owned()).unwrap_or_else(|_| a.value.to_string())
        })
    };
    let is_history = |s: &str| {
        let l = s.to_lowercase();
        l.contains("history") || l.contains("historique") || l.contains("historial")
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == "NODE" => {
                let name = attr(&e, "Name").unwrap_or_default();
                if attr(&e, "Type").as_deref() == Some("1") {
                    let in_history = stack.iter().any(|n| is_history(n)) || is_history(&name);
                    current = match (in_history || date_in(&name).is_some(), date_in(&name)) {
                        (true, Some(d)) => Some((d, Vec::new())),
                        _ => None,
                    };
                }
                stack.push(name);
            }
            Ok(Event::Empty(e)) if e.name().as_ref() == "TRACK" => {
                if let (Some((_, list)), Some(key)) = (current.as_mut(), attr(&e, "Key")) {
                    if let Some(p) = locations.get(&key).and_then(|l| crate::rekordbox::decode_location(l)) {
                        list.push(std::path::PathBuf::from(p));
                    }
                }
            }
            Ok(Event::End(e)) if e.name().as_ref() == "NODE" => {
                stack.pop();
                if let Some(c) = current.take() {
                    if !c.1.is_empty() {
                        out.push(c);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(AppError::InvalidSetting(format!("XML illisible : {e}"))),
            _ => {}
        }
    }
    Ok(out)
}

/// Importe un historique : XML de Rekordbox (toutes les listes d'historique) ou une liste
/// exportée en M3U8 / TXT (date prise dans le nom du fichier, sinon sa date de modification).
pub fn import_history(conn: &Connection, mounts: &HashMap<i64, std::path::PathBuf>, file: &std::path::Path) -> Result<HistoryReport, AppError> {
    let text = std::fs::read(file)?;
    let text = String::from_utf8_lossy(&text).into_owned();
    let sessions: Vec<(String, Vec<std::path::PathBuf>)> = if text.contains("<DJ_PLAYLISTS") {
        parse_xml_history(&text)?
    } else {
        let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let date = date_in(&stem).unwrap_or_else(|| {
            std::fs::metadata(file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| civil((d.as_secs() / 86_400) as i64))
                .unwrap_or_else(crate::db::today)
        });
        vec![(date, parse_history_file(&text))]
    };
    let mut report = HistoryReport { sessions: sessions.len(), ..Default::default() };
    for (date, paths) in sessions {
        let ids = match_paths(conn, mounts, &paths)?;
        let mut plays = Vec::new();
        for (p, id) in paths.iter().zip(ids) {
            match id {
                Some(id) => plays.push((id, date.clone())),
                None => {
                    report.unmatched += 1;
                    if report.unmatched_examples.len() < 5 {
                        report.unmatched_examples.push(p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                    }
                }
            }
        }
        report.plays += record_plays(conn, &plays, "rekordbox")?;
    }
    Ok(report)
}

/// Date « AAAA-MM-JJ » d'un nombre de jours depuis le 1er janvier 1970.
fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Lit une playlist d'historique exportée par Rekordbox (M3U8 ou TXT) : chemins des fichiers.
pub fn parse_history_file(text: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim().trim_start_matches('\u{feff}');
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        // TXT de Rekordbox : colonnes séparées par des tabulations, dont « Location ».
        let field = l.split('\t').find(|c| c.contains(['/', '\\']) && (c.contains('.'))).unwrap_or(l);
        let f = field.trim();
        if f.starts_with("file://") {
            if let Some(p) = crate::rekordbox::decode_location(f) {
                out.push(std::path::PathBuf::from(p));
            }
        } else if f.contains(['/', '\\']) {
            out.push(std::path::PathBuf::from(f));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(conn: &Connection, title: &str, artist: &str, bpm: f64, key: &str, genre: &str) -> i64 {
        conn.execute("INSERT OR IGNORE INTO volumes (id, uid, label) VALUES (1, 'u', 'D')", []).ok();
        conn.execute(
            "INSERT INTO tracks (volume_id, rel_path, file_name, size, mtime, format, status, title, main_artist, bpm, musical_key, genre)
             VALUES (1, ?1, ?1, 1000, 1, 'mp3', 'ok', ?2, ?3, ?4, ?5, ?6)",
            params![format!("{title}.mp3"), title, artist, bpm, key, genre],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn roue_camelot() {
        assert_eq!(key_compat("Am", "Am"), "same"); // 8A
        assert_eq!(key_compat("Am", "Em"), "adjacent"); // 8A → 9A
        assert_eq!(key_compat("Am", "Dm"), "adjacent"); // 8A → 7A
        assert_eq!(key_compat("Am", "C"), "relative"); // 8A → 8B
        assert_eq!(key_compat("Am", "Bm"), "energy"); // 8A → 10A
        assert_eq!(key_compat("Am", "F#"), "clash");
        assert_eq!(key_compat("Am", ""), "unknown");
        assert_eq!(key_compat("Bm", "Am"), "clash", "−2 n'est pas une montée d'énergie");
    }

    #[test]
    fn ecart_bpm_avec_double_tempo() {
        assert!((bpm_diff(124.0, 126.0).unwrap() - 1.6129).abs() < 0.01);
        assert!(bpm_diff(128.0, 64.0).unwrap().abs() < 0.001);
        assert!(bpm_diff(0.0, 120.0).is_none());
    }

    #[test]
    fn set_ordre_et_suggestions() {
        let conn = crate::db::open_in_memory().unwrap();
        let a = track(&conn, "A", "X", 124.0, "Am", "House");
        let b = track(&conn, "B", "Y", 125.0, "Em", "House");
        let c = track(&conn, "C", "Z", 126.0, "F#", "House"); // tonalité incompatible
        let d = track(&conn, "D", "W", 140.0, "Am", "House"); // trop rapide
        let e = track(&conn, "E", "V", 123.0, "Am", "Techno");
        conn.execute("INSERT INTO playlists (name, kind) VALUES ('Mon set', 'set')", []).unwrap();
        let p = conn.last_insert_rowid();
        crate::playlists::add_tracks(&conn, p, &[a, b]).unwrap();
        let s = set_tracks(&conn, p).unwrap();
        assert_eq!(s[1].key_compat.as_deref(), Some("adjacent"));
        set_move(&conn, p, b, 0).unwrap();
        assert_eq!(set_tracks(&conn, p).unwrap()[0].id, b);

        let sug = suggestions(&conn, a, &[a, b], 6.0, 10).unwrap();
        let ids: Vec<i64> = sug.iter().map(|x| x.track.id).collect();
        assert_eq!(ids, vec![e], "C (tonalité) et D (BPM) exclus, A et B déjà dans le set");
        assert!(!ids.contains(&c) && !ids.contains(&d));
    }

    #[test]
    fn etiquettes_et_statistiques() {
        let conn = crate::db::open_in_memory().unwrap();
        let a = track(&conn, "A", "X", 124.0, "Am", "House");
        let b = track(&conn, "B", "Y", 125.0, "Em", "Techno");
        let t = tag_create(&conn, "Mariage", "pink").unwrap();
        assert_eq!(tag_create(&conn, " mariage ", "red").unwrap(), t, "même nom, même étiquette");
        assert_eq!(tag_apply(&conn, t, &[a, b, a], true).unwrap(), 2);
        assert_eq!(tags_list(&conn).unwrap()[0].count, 2);
        let q = crate::library::query::Query { tag: Some(t), ..Default::default() };
        assert_eq!(crate::library::query::ids(&conn, &q).unwrap().len(), 2);
        tag_apply(&conn, t, &[b], false).unwrap();
        assert_eq!(crate::library::query::ids(&conn, &q).unwrap(), vec![a]);
        let rows = crate::library::query::rows(&conn, &[a], &HashMap::new()).unwrap();
        assert_eq!(rows[0].user_tags[0].name, "Mariage");

        record_plays(&conn, &[(a, "2026-11-01".into()), (a, "2026-11-08".into()), (a, "2026-11-08".into())], "rekordbox").unwrap();
        let st = stats(&conn).unwrap();
        assert_eq!(st.tracks, 2);
        assert_eq!(st.most_played[0].count, 2, "un passage par soirée");
        assert_eq!(st.keys[0].label, "8A");
        let never = crate::library::query::Query { played: Some("never".into()), ..Default::default() };
        assert_eq!(crate::library::query::ids(&conn, &never).unwrap(), vec![b]);
        tag_delete(&conn, t).unwrap();
        assert!(tags_list(&conn).unwrap().is_empty());
    }

    #[test]
    fn historique_du_xml_rekordbox() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><DJ_PLAYLISTS Version="1.0.0">
<COLLECTION Entries="2">
<TRACK TrackID="1" Name="A" Location="file://localhost/E:/MUSICS/ALL/a.mp3"/>
<TRACK TrackID="2" Name="B" Location="file://localhost/E:/MUSICS/ALL/b%20c.mp3"/>
</COLLECTION>
<PLAYLISTS><NODE Type="0" Name="ROOT" Count="2">
<NODE Type="0" Name="HISTORY" Count="1">
<NODE Name="HISTORY 2026-11-27" Type="1" KeyType="0" Entries="2"><TRACK Key="1"/><TRACK Key="2"/></NODE>
</NODE>
<NODE Name="Mariage" Type="1" KeyType="0" Entries="1"><TRACK Key="1"/></NODE>
</NODE></PLAYLISTS></DJ_PLAYLISTS>"#;
        let h = parse_xml_history(xml).unwrap();
        assert_eq!(h.len(), 1, "« Mariage » n'est pas un historique");
        assert_eq!(h[0].0, "2026-11-27");
        assert_eq!(h[0].1.len(), 2);
        assert_eq!(civil(20_419), "2025-11-27");
    }

    #[test]
    fn dates_et_fichiers_d_historique() {
        assert_eq!(date_in("HISTORY 2026-11-27").as_deref(), Some("2026-11-27"));
        assert_eq!(date_in("Historique 2026-11-27 (2)").as_deref(), Some("2026-11-27"));
        assert_eq!(date_in("27/11/2026").as_deref(), Some("2026-11-27"));
        assert_eq!(date_in("Ma playlist"), None);
        let m3u = "#EXTM3U\n#EXTINF:200,A - B\nE:\\MUSICS\\ALL\\a.mp3\n\nfile://localhost/E:/MUSICS/ALL/b%20c.mp3\n";
        let p = parse_history_file(m3u);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].file_name().unwrap().to_string_lossy(), "b c.mp3");
    }
}
