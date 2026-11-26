//! Base de données locale SQLite et ses migrations.
//!
//! Chaque migration est une étape numérotée. Le numéro de la dernière étape
//! appliquée est rangé dans `PRAGMA user_version`. On n'efface ni ne modifie
//! jamais une migration déjà publiée : on en ajoute une nouvelle à la fin.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::AppError;

/// Liste ordonnée des migrations. L'index + 1 donne le numéro de version.
const MIGRATIONS: &[&str] = &[
    // 1 — Réglages de l'application (thème, langue, etc.).
    "CREATE TABLE settings (
        key        TEXT PRIMARY KEY NOT NULL,
        value      TEXT NOT NULL,
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    );",
];

/// Numéro de la dernière migration connue de cette version de l'application.
pub fn latest_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// Version actuelle d'une base ouverte.
pub fn current_version(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

/// Réglages de connexion communs.
fn configure(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    Ok(())
}

/// Applique les migrations manquantes, chacune dans sa propre transaction.
/// Refuse d'ouvrir une base créée par une version plus récente de l'application,
/// pour ne pas l'abîmer.
pub fn migrate(conn: &mut Connection) -> Result<(), AppError> {
    let current = current_version(conn)?;
    let latest = latest_version();
    if current > latest {
        return Err(AppError::DatabaseTooNew { found: current, supported: latest });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = index as i64 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        log::info!("Base de données : migration {version} appliquée");
    }
    Ok(())
}

/// Ouvre (ou crée) la base dans le dossier de données de l'application.
/// Avant de migrer une base existante, une copie de sécurité est faite à côté.
pub fn open(data_dir: &Path) -> Result<(Connection, PathBuf), AppError> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join("library.sqlite");
    let mut conn = Connection::open(&path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    configure(&conn)?;

    let current = current_version(&conn)?;
    if current > 0 && current < latest_version() {
        let backup = data_dir.join(format!("library.before-v{}.sqlite", latest_version()));
        if !backup.exists() {
            conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
            log::info!("Copie de sécurité avant migration : {}", backup.display());
        }
    }
    migrate(&mut conn)?;
    Ok((conn, path))
}

/// Base en mémoire, pour les tests.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, AppError> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nouvelle_base_est_a_la_derniere_version() {
        let conn = open_in_memory().unwrap();
        assert_eq!(current_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn migrer_deux_fois_ne_change_rien() {
        let mut conn = open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn base_plus_recente_refusee() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", latest_version() + 1).unwrap();
        assert!(matches!(migrate(&mut conn), Err(AppError::DatabaseTooNew { .. })));
    }

    #[test]
    fn base_sur_disque_rouverte_sans_perte() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (conn, _) = open(dir.path()).unwrap();
            conn.execute("INSERT INTO settings (key, value) VALUES ('theme', 'dark')", [])
                .unwrap();
        }
        let (conn, path) = open(dir.path()).unwrap();
        assert!(path.exists());
        let value: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'theme'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "dark");
    }
}
