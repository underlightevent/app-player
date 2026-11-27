//! Playlists (liste de morceaux choisis à la main) et filtres intelligents (règles :
//! « genre contient Techno », « note au moins 4 »…). Exportables en M3U et dans le XML Rekordbox.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    /// Toutes les règles (vrai) ou au moins une (faux).
    #[serde(default = "yes")]
    pub match_all: bool,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: i64,
    pub name: String,
    /// « static » ou « smart »
    pub kind: String,
    pub rules: Option<Rules>,
    pub count: i64,
}

/// Colonne SQL d'un champ de règle (liste fermée : aucune injection possible).
fn column(field: &str) -> Option<(&'static str, &'static str)> {
    Some(match field {
        "title" => ("t.title", "text"),
        "artist" => ("t.artists", "text"),
        "album" => ("t.album", "text"),
        "genre" => ("t.genre", "text"),
        "label" => ("t.label", "text"),
        "version" => ("t.version", "text"),
        "fileName" => ("t.file_name", "text"),
        "key" => ("t.musical_key", "text"),
        "format" => ("t.format", "text"),
        "year" => ("t.year", "number"),
        "bpm" => ("t.bpm", "number"),
        "rating" => ("t.rating", "number"),
        "duration" => ("(t.duration_ms / 1000)", "number"),
        "rekordbox" => ("(t.id IN (SELECT track_id FROM rekordbox_tracks))", "bool"),
        "hasCover" => ("(t.cover_count > 0)", "bool"),
        // Phase 6
        "userTag" => ("(SELECT group_concat(g.name, ' | ') FROM track_user_tags tt JOIN user_tags g ON g.id = tt.tag_id WHERE tt.track_id = t.id)", "text"),
        "plays" => ("(SELECT count(*) FROM plays p WHERE p.track_id = t.id)", "number"),
        "energy" => ("(SELECT energy FROM analysis a WHERE a.track_id = t.id)", "number"),
        "lufs" => ("(SELECT lufs FROM loudness l WHERE l.track_id = t.id)", "number"),
        _ => return None,
    })
}

/// Condition SQL d'un filtre intelligent, avec ses paramètres.
pub fn rules_sql(r: &Rules) -> Result<(String, Vec<rusqlite::types::Value>), AppError> {
    let mut parts = Vec::new();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    for rule in &r.rules {
        let (col, kind) = column(&rule.field).ok_or_else(|| AppError::InvalidSetting(format!("champ inconnu : {}", rule.field)))?;
        let v = rule.value.trim().to_string();
        let part = match (kind, rule.op.as_str()) {
            ("text", "contains") => {
                args.push(format!("%{}%", v.replace('%', "")).into());
                format!("coalesce({col}, '') LIKE ? ")
            }
            ("text", "not_contains") => {
                args.push(format!("%{}%", v.replace('%', "")).into());
                format!("coalesce({col}, '') NOT LIKE ? ")
            }
            ("text", "is") => {
                args.push(v.into());
                format!("coalesce({col}, '') = ? COLLATE NOCASE")
            }
            ("text", "is_not") => {
                args.push(v.into());
                format!("coalesce({col}, '') != ? COLLATE NOCASE")
            }
            ("text", "empty") => format!("coalesce({col}, '') = ''"),
            ("text", "not_empty") => format!("coalesce({col}, '') != ''"),
            ("number", op @ ("eq" | "gte" | "lte")) => {
                let n: f64 = v.replace(',', ".").parse().map_err(|_| AppError::InvalidSetting(format!("nombre attendu : {v}")))?;
                args.push(n.into());
                let sym = match op {
                    "eq" => "=",
                    "gte" => ">=",
                    _ => "<=",
                };
                format!("{col} {sym} ?")
            }
            ("number", "empty") => format!("({col} IS NULL OR {col} = 0)"),
            ("bool", "is_true") => col.to_string(),
            ("bool", "is_false") => format!("NOT {col}"),
            _ => return Err(AppError::InvalidSetting(format!("règle invalide : {} {}", rule.field, rule.op))),
        };
        parts.push(format!("({part})"));
    }
    if parts.is_empty() {
        return Ok(("1".into(), args));
    }
    Ok((parts.join(if r.match_all { " AND " } else { " OR " }), args))
}

fn info(conn: &Connection, id: i64) -> Result<Option<PlaylistInfo>, AppError> {
    let row: Option<(i64, String, String, Option<String>)> = conn
        .query_row("SELECT id, name, kind, rules FROM playlists WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    let Some((id, name, kind, rules)) = row else { return Ok(None) };
    let rules: Option<Rules> = rules.and_then(|r| serde_json::from_str(&r).ok());
    let mut p = PlaylistInfo { id, name, kind, rules, count: 0 };
    p.count = track_ids(conn, &p)?.len() as i64;
    Ok(Some(p))
}

pub fn list(conn: &Connection) -> Result<Vec<PlaylistInfo>, AppError> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT id FROM playlists ORDER BY name COLLATE NOCASE, id")?;
        let v = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
        v
    };
    let mut out = Vec::new();
    for id in ids {
        if let Some(p) = info(conn, id)? {
            out.push(p);
        }
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> Result<PlaylistInfo, AppError> {
    info(conn, id)?.ok_or_else(|| AppError::InvalidSetting("playlist introuvable".into()))
}

fn clean_name(name: &str) -> Result<String, AppError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 200 {
        return Err(AppError::InvalidSetting("nom de playlist invalide".into()));
    }
    Ok(n.to_string())
}

pub fn create(conn: &Connection, name: &str, rules: Option<&Rules>) -> Result<i64, AppError> {
    if let Some(r) = rules {
        rules_sql(r)?; // règles vérifiées avant d'être gardées
    }
    conn.execute(
        "INSERT INTO playlists (name, kind, rules) VALUES (?1, ?2, ?3)",
        params![clean_name(name)?, if rules.is_some() { "smart" } else { "static" }, rules.map(|r| serde_json::to_string(r).unwrap_or_default())],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Nouveau set (liste ordonnée de l’écran Sets, phase 6).
pub fn create_set(conn: &Connection, name: &str) -> Result<i64, AppError> {
    conn.execute("INSERT INTO playlists (name, kind) VALUES (?1, 'set')", [clean_name(name)?])?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, name: Option<&str>, rules: Option<&Rules>) -> Result<(), AppError> {
    if let Some(n) = name {
        conn.execute("UPDATE playlists SET name = ?2 WHERE id = ?1", params![id, clean_name(n)?])?;
    }
    if let Some(r) = rules {
        rules_sql(r)?;
        conn.execute("UPDATE playlists SET rules = ?2 WHERE id = ?1 AND kind = 'smart'", params![id, serde_json::to_string(r).unwrap_or_default()])?;
    }
    Ok(())
}

/// Supprime la playlist (jamais les morceaux ni les fichiers).
pub fn delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", [id])?;
    Ok(())
}

pub fn add_tracks(conn: &Connection, id: i64, ids: &[i64]) -> Result<usize, AppError> {
    let kind: String = conn.query_row("SELECT kind FROM playlists WHERE id = ?1", [id], |r| r.get(0))?;
    if kind == "smart" {
        return Err(AppError::InvalidSetting("un filtre intelligent se remplit tout seul".into()));
    }
    let mut pos: i64 = conn.query_row("SELECT coalesce(max(position), 0) FROM playlist_tracks WHERE playlist_id = ?1", [id], |r| r.get(0))?;
    let mut added = 0;
    for t in ids {
        pos += 1;
        added += conn.execute("INSERT OR IGNORE INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)", params![id, t, pos])?;
    }
    Ok(added)
}

pub fn remove_tracks(conn: &Connection, id: i64, ids: &[i64]) -> Result<(), AppError> {
    for t in ids {
        conn.execute("DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2", params![id, t])?;
    }
    Ok(())
}

/// Morceaux d'une playlist, dans son ordre (morceaux en quarantaine exclus).
pub fn track_ids(conn: &Connection, p: &PlaylistInfo) -> Result<Vec<i64>, AppError> {
    if p.kind == "smart" {
        let rules = p.rules.clone().unwrap_or_default();
        let (cond, args) = rules_sql(&rules)?;
        let sql = format!(
            "SELECT t.id FROM tracks t WHERE t.status != 'quarantined' AND ({cond})
             ORDER BY (t.main_artist IS NULL), t.main_artist COLLATE NOCASE, t.title COLLATE NOCASE, t.id"
        );
        let mut st = conn.prepare(&sql)?;
        let v = st.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
        return Ok(v);
    }
    let mut st = conn.prepare(
        "SELECT p.track_id FROM playlist_tracks p JOIN tracks t ON t.id = p.track_id
         WHERE p.playlist_id = ?1 AND t.status != 'quarantined' ORDER BY p.position",
    )?;
    let v = st.query_map([p.id], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn fill(conn: &Connection) {
        conn.execute("INSERT INTO volumes (id, uid, label) VALUES (1, 'v', 'V')", []).unwrap();
        for (i, (genre, rating, bpm)) in [("Techno", 5, 130.0), ("House", 3, 124.0), ("Techno", 2, 128.0), ("Tech House", 4, 126.0)].iter().enumerate() {
            conn.execute(
                "INSERT INTO tracks (volume_id, rel_path, file_name, genre, rating, bpm, title) VALUES (1, ?1, ?1, ?2, ?3, ?4, ?5)",
                params![format!("f{i}.mp3"), genre, rating, bpm, format!("Titre {i}")],
            )
            .unwrap();
        }
    }

    #[test]
    fn filtre_intelligent() {
        let conn = open_in_memory().unwrap();
        fill(&conn);
        let rules = Rules {
            match_all: true,
            rules: vec![
                Rule { field: "genre".into(), op: "contains".into(), value: "techno".into() },
                Rule { field: "rating".into(), op: "gte".into(), value: "4".into() },
            ],
        };
        let id = create(&conn, "Techno 4★+", Some(&rules)).unwrap();
        let p = get(&conn, id).unwrap();
        assert_eq!(p.kind, "smart");
        assert_eq!(p.count, 1, "« Techno » 5★ seulement (Tech House ne contient pas « techno »)");
        let any = Rules { match_all: false, ..rules.clone() };
        update(&conn, id, Some("Techno ou 4★+"), Some(&any)).unwrap();
        assert_eq!(get(&conn, id).unwrap().count, 3);
        // Règle invalide refusée (aucune injection possible).
        let bad = Rules { match_all: true, rules: vec![Rule { field: "genre; DROP TABLE tracks".into(), op: "is".into(), value: "x".into() }] };
        assert!(create(&conn, "x", Some(&bad)).is_err());
        let bad = Rules { match_all: true, rules: vec![Rule { field: "bpm".into(), op: "gte".into(), value: "beaucoup".into() }] };
        assert!(create(&conn, "x", Some(&bad)).is_err());
    }

    #[test]
    fn playlist_simple() {
        let conn = open_in_memory().unwrap();
        fill(&conn);
        let id = create(&conn, "Warm-up", None).unwrap();
        assert_eq!(add_tracks(&conn, id, &[3, 1, 3]).unwrap(), 2, "pas de doublon dans une playlist");
        let p = get(&conn, id).unwrap();
        assert_eq!(track_ids(&conn, &p).unwrap(), vec![3, 1], "ordre d'ajout gardé");
        remove_tracks(&conn, id, &[3]).unwrap();
        assert_eq!(get(&conn, id).unwrap().count, 1);
        assert!(create(&conn, "   ", None).is_err());
        delete(&conn, id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
        let n: i64 = conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 4, "supprimer une playlist ne touche pas aux morceaux");
    }
}
