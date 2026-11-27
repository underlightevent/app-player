//! Réglages de l'application, rangés dans la table `settings`.
//!
//! Seules les clés connues sont acceptées, avec une liste de valeurs permises,
//! pour qu'une erreur de l'interface ne puisse pas enregistrer n'importe quoi.

use std::collections::BTreeMap;

use rusqlite::{params, Connection};

use crate::error::AppError;

/// (clé, valeurs permises, valeur par défaut)
const KNOWN: &[(&str, &[&str], &str)] = &[
    ("theme", &["light", "dark", "system"], "light"),
    ("language", &["fr", "en"], "fr"),
    ("onboarding_done", &["true", "false"], "false"),
    ("table_density", &["compact", "comfortable"], "compact"),
    ("key_notation", &["camelot", "musical"], "camelot"),
    ("version_in_title", &["true", "false"], "true"),
    ("artist_in_title", &["true", "false"], "false"),
    ("artist_separator", &["comma", "ampersand", "semicolon"], "comma"),
    ("bpm_decimals", &["true", "false"], "false"),
    ("single_cover", &["true", "false"], "true"),
    ("cover_size", &["500", "600", "1000", "1400"], "1000"),
    // Recherche en ligne (docs/SOURCES.md : Deezer et iTunes désactivés par défaut)
    ("src_musicbrainz", &["true", "false"], "true"),
    ("src_coverartarchive", &["true", "false"], "true"),
    ("src_acoustid", &["true", "false"], "true"),
    ("src_deezer", &["true", "false"], "false"),
    ("src_itunes", &["true", "false"], "false"),
    ("auto_threshold", &["70", "75", "80", "85", "90", "95", "100"], "90"),
    ("review_threshold", &["40", "45", "50", "55", "60", "65", "70"], "60"),
    ("replace_existing", &["true", "false"], "false"),
    ("auto_rename", &["inbox", "library", "never"], "inbox"),
    // Doublons et qualité (phase 3)
    (
        "duplicate_order",
        &["wav,aiff,flac", "wav,flac,aiff", "aiff,wav,flac", "aiff,flac,wav", "flac,wav,aiff", "flac,aiff,wav"],
        "wav,aiff,flac",
    ),
    ("low_bitrate", &["128", "160", "192", "256", "320"], "320"),
    // Rekordbox et mises à jour (phase 4)
    ("rekordbox_protected", &["true", "false"], "true"),
    ("update_check", &["true", "false"], "true"),
    // Veille musicale (phase 5) : sources désactivées par défaut.
    ("watch_apple", &["true", "false"], "false"),
    ("watch_deezer", &["true", "false"], "false"),
    ("watch_musicbrainz", &["true", "false"], "false"),
    // Extraits de 30 s (au clic sur ▶ seulement) et site d'ouverture des titres.
    ("watch_preview", &["deezer", "apple", "off"], "deezer"),
    (
        "watch_open_with",
        &["source", "apple", "deezer", "spotify", "youtube", "youtube_music", "soundcloud", "beatport"],
        "source",
    ),
];

/// Réglages en texte libre (modèles, règles), avec leur valeur par défaut.
const FREE_TEXT: &[(&str, &str)] = &[
    ("rename_template", crate::rename::template::DEFAULT_TEMPLATE),
    ("rename_templates", "[]"),
    ("parse_pattern", "{artistes} - {titre}"),
    ("clean_rules", ""),
    ("online_contact", ""),
    ("inbox_last_target", ""),
    ("spotify_client_id", ""),
    ("spotify_user", ""),
    ("watch_last", ""),
];
const MAX_FREE_TEXT: usize = 20_000;

fn validate(key: &str, value: &str) -> Result<(), AppError> {
    if FREE_TEXT.iter().any(|(k, _)| *k == key) {
        return if value.len() <= MAX_FREE_TEXT { Ok(()) } else { Err(AppError::InvalidSetting(format!("{key} trop long"))) };
    }
    match KNOWN.iter().find(|(k, _, _)| *k == key) {
        Some((_, allowed, _)) if allowed.contains(&value) => Ok(()),
        Some(_) => Err(AppError::InvalidSetting(format!("{key} = {value}"))),
        None => Err(AppError::InvalidSetting(key.to_string())),
    }
}

/// Tous les réglages, valeurs par défaut comprises.
pub fn get_all(conn: &Connection) -> Result<BTreeMap<String, String>, AppError> {
    let mut map: BTreeMap<String, String> = KNOWN
        .iter()
        .map(|(k, _, d)| (k.to_string(), d.to_string()))
        .chain(FREE_TEXT.iter().map(|(k, d)| (k.to_string(), d.to_string())))
        .collect();
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (key, value) = row?;
        // Une valeur devenue invalide (ancienne version) est ignorée au profit du défaut.
        if validate(&key, &value).is_ok() {
            map.insert(key, value);
        }
    }
    Ok(map)
}

/// Valeur d'un réglage (valeur par défaut comprise).
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    Ok(get_all(conn)?.remove(key))
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    validate(key, value)?;
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, unixepoch())
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn valeurs_par_defaut() {
        let conn = open_in_memory().unwrap();
        let all = get_all(&conn).unwrap();
        assert_eq!(all["theme"], "light");
        assert_eq!(all["language"], "fr");
    }

    #[test]
    fn enregistrer_puis_relire() {
        let conn = open_in_memory().unwrap();
        set(&conn, "theme", "dark").unwrap();
        set(&conn, "language", "en").unwrap();
        set(&conn, "theme", "system").unwrap();
        let all = get_all(&conn).unwrap();
        assert_eq!(all["theme"], "system");
        assert_eq!(all["language"], "en");
    }

    #[test]
    fn valeur_ou_cle_inconnue_refusee() {
        let conn = open_in_memory().unwrap();
        assert!(set(&conn, "theme", "rose").is_err());
        assert!(set(&conn, "inconnu", "x").is_err());
        assert_eq!(get_all(&conn).unwrap()["theme"], "light");
    }
}
