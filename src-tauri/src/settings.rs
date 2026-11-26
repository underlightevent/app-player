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
];

fn validate(key: &str, value: &str) -> Result<(), AppError> {
    match KNOWN.iter().find(|(k, _, _)| *k == key) {
        Some((_, allowed, _)) if allowed.contains(&value) => Ok(()),
        Some(_) => Err(AppError::InvalidSetting(format!("{key} = {value}"))),
        None => Err(AppError::InvalidSetting(key.to_string())),
    }
}

/// Tous les réglages, valeurs par défaut comprises.
pub fn get_all(conn: &Connection) -> Result<BTreeMap<String, String>, AppError> {
    let mut map: BTreeMap<String, String> =
        KNOWN.iter().map(|(k, _, d)| (k.to_string(), d.to_string())).collect();
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
