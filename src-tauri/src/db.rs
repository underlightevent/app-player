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
    // 2 — Bibliothèque : volumes, racines, morceaux, artistes, état des champs.
    "CREATE TABLE volumes (
        id         INTEGER PRIMARY KEY,
        uid        TEXT NOT NULL UNIQUE,
        label      TEXT NOT NULL DEFAULT '',
        last_mount TEXT
    );
    CREATE TABLE roots (
        id        INTEGER PRIMARY KEY,
        volume_id INTEGER NOT NULL REFERENCES volumes(id),
        rel_path  TEXT NOT NULL,
        kind      TEXT NOT NULL DEFAULT 'library',
        added_at  INTEGER NOT NULL DEFAULT (unixepoch()),
        UNIQUE (volume_id, rel_path)
    );
    CREATE TABLE tracks (
        id           INTEGER PRIMARY KEY,
        volume_id    INTEGER NOT NULL REFERENCES volumes(id),
        rel_path     TEXT NOT NULL,
        file_name    TEXT NOT NULL,
        root_id      INTEGER REFERENCES roots(id) ON DELETE SET NULL,
        size         INTEGER NOT NULL DEFAULT 0,
        mtime        INTEGER NOT NULL DEFAULT 0,
        format       TEXT NOT NULL DEFAULT '',
        codec        TEXT,
        bitrate      INTEGER,
        sample_rate  INTEGER,
        bit_depth    INTEGER,
        channels     INTEGER,
        duration_ms  INTEGER,
        audio_hash   TEXT,
        title        TEXT,
        version      TEXT,
        artists      TEXT,
        main_artist  TEXT,
        album        TEXT,
        year         INTEGER,
        genre        TEXT,
        bpm          REAL,
        musical_key  TEXT,
        rating       INTEGER NOT NULL DEFAULT 0,
        remixer      TEXT,
        label        TEXT,
        cover_count  INTEGER NOT NULL DEFAULT 0,
        cover_width  INTEGER,
        cover_height INTEGER,
        wav_desync   INTEGER NOT NULL DEFAULT 0,
        status       TEXT NOT NULL DEFAULT 'ok',
        error        TEXT,
        indexed_at   INTEGER,
        UNIQUE (volume_id, rel_path)
    );
    CREATE INDEX tracks_root ON tracks(root_id);
    CREATE INDEX tracks_status ON tracks(status);
    CREATE INDEX tracks_hash ON tracks(audio_hash);
    CREATE TABLE track_artists (
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        name     TEXT NOT NULL,
        PRIMARY KEY (track_id, position)
    );
    CREATE TABLE field_state (
        track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        field    TEXT NOT NULL,
        locked   INTEGER NOT NULL DEFAULT 0,
        source   TEXT NOT NULL DEFAULT 'file',
        PRIMARY KEY (track_id, field)
    );
    CREATE TABLE artist_exceptions (
        name TEXT PRIMARY KEY COLLATE NOCASE
    );
    INSERT INTO artist_exceptions(name) VALUES
        ('Above & Beyond'), ('Simon & Garfunkel'), ('Earth, Wind & Fire'),
        ('Crosby, Stills, Nash & Young'), ('Chase & Status'), ('Sigma & Example'),
        ('Mumford & Sons'), ('Florence + The Machine'), ('Lost & Found');
    CREATE VIRTUAL TABLE tracks_fts USING fts5(
        title, version, artists, album, genre, file_name,
        content='tracks', content_rowid='id',
        tokenize='unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER tracks_ai AFTER INSERT ON tracks BEGIN
        INSERT INTO tracks_fts(rowid, title, version, artists, album, genre, file_name)
        VALUES (new.id, new.title, new.version, new.artists, new.album, new.genre, new.file_name);
    END;
    CREATE TRIGGER tracks_ad AFTER DELETE ON tracks BEGIN
        INSERT INTO tracks_fts(tracks_fts, rowid, title, version, artists, album, genre, file_name)
        VALUES ('delete', old.id, old.title, old.version, old.artists, old.album, old.genre, old.file_name);
    END;
    CREATE TRIGGER tracks_au AFTER UPDATE OF title, version, artists, album, genre, file_name ON tracks BEGIN
        INSERT INTO tracks_fts(tracks_fts, rowid, title, version, artists, album, genre, file_name)
        VALUES ('delete', old.id, old.title, old.version, old.artists, old.album, old.genre, old.file_name);
        INSERT INTO tracks_fts(rowid, title, version, artists, album, genre, file_name)
        VALUES (new.id, new.title, new.version, new.artists, new.album, new.genre, new.file_name);
    END;",
    // 3 — Historique des opérations (annulation, même après redémarrage).
    "CREATE TABLE history_batches (
        id         INTEGER PRIMARY KEY,
        label      TEXT NOT NULL,
        detail     TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        undone     INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE history_ops (
        id       INTEGER PRIMARY KEY,
        batch_id INTEGER NOT NULL REFERENCES history_batches(id) ON DELETE CASCADE,
        kind     TEXT NOT NULL,
        track_id INTEGER,
        before   TEXT NOT NULL,
        after    TEXT NOT NULL
    );
    CREATE INDEX history_ops_batch ON history_ops(batch_id);
    CREATE TABLE history_blobs (
        hash TEXT PRIMARY KEY,
        mime TEXT NOT NULL,
        data BLOB NOT NULL
    );",
    // 4 — Formes d'onde en cache.
    "CREATE TABLE waveforms (
        track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        size     INTEGER NOT NULL,
        mtime    INTEGER NOT NULL,
        data     BLOB NOT NULL
    );",
    // 5 — Recherche en ligne : empreintes, cache, journal des appels, résultats, file d'attente, Inbox.
    "CREATE TABLE fingerprints (
        track_id    INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        size        INTEGER NOT NULL,
        mtime       INTEGER NOT NULL,
        duration_s  INTEGER NOT NULL,
        fingerprint TEXT NOT NULL,
        raw         BLOB
    );
    CREATE TABLE online_cache (
        key        TEXT PRIMARY KEY,
        body       BLOB NOT NULL,
        fetched_at INTEGER NOT NULL
    );
    CREATE TABLE online_calls (
        id          INTEGER PRIMARY KEY,
        ts_ms       INTEGER NOT NULL,
        source      TEXT NOT NULL,
        status      INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL,
        cached      INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX online_calls_ts ON online_calls(ts_ms);
    CREATE TABLE online_results (
        track_id   INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        status     TEXT NOT NULL,
        score      INTEGER NOT NULL DEFAULT 0,
        candidates TEXT NOT NULL DEFAULT '[]',
        applied    TEXT,
        detail     TEXT,
        updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE INDEX online_results_status ON online_results(status);
    CREATE TABLE online_queue (
        track_id     INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        requested_at INTEGER NOT NULL DEFAULT (unixepoch()),
        attempts     INTEGER NOT NULL DEFAULT 0
    );
    CREATE TABLE inbox_items (
        track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        state    TEXT NOT NULL DEFAULT 'pending',
        added_at INTEGER NOT NULL DEFAULT (unixepoch())
    );",
    // 6 — Doublons, quarantaine et contrôle qualité (phase 3).
    "CREATE TABLE quarantine (
        id        INTEGER PRIMARY KEY,
        track_id  INTEGER UNIQUE REFERENCES tracks(id) ON DELETE SET NULL,
        volume_id INTEGER NOT NULL REFERENCES volumes(id),
        orig_rel  TEXT NOT NULL,
        quar_rel  TEXT NOT NULL,
        size      INTEGER NOT NULL DEFAULT 0,
        reason    TEXT,
        added_at  INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE duplicate_groups (
        id            INTEGER PRIMARY KEY,
        kind          TEXT NOT NULL,
        keeper_id     INTEGER,
        keeper_manual INTEGER NOT NULL DEFAULT 0,
        created_at    INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE duplicate_members (
        group_id   INTEGER NOT NULL REFERENCES duplicate_groups(id) ON DELETE CASCADE,
        track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        similarity REAL,
        PRIMARY KEY (group_id, track_id)
    );
    CREATE INDEX duplicate_members_track ON duplicate_members(track_id);
    CREATE TABLE not_duplicates (
        a INTEGER NOT NULL,
        b INTEGER NOT NULL,
        PRIMARY KEY (a, b)
    );
    CREATE TABLE quality (
        track_id     INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        size         INTEGER NOT NULL,
        mtime        INTEGER NOT NULL,
        verdict      TEXT NOT NULL,
        cutoff_hz    INTEGER,
        decode_errors INTEGER NOT NULL DEFAULT 0,
        truncated    INTEGER NOT NULL DEFAULT 0,
        detail       TEXT,
        analyzed_at  INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE INDEX quality_verdict ON quality(verdict);",
    // 7 — Rekordbox, journal des chemins, playlists et filtres intelligents (phase 4).
    "CREATE TABLE rekordbox_tracks (
        track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        rb_id    TEXT,
        location TEXT NOT NULL
    );
    CREATE TABLE rekordbox_import (
        id          INTEGER PRIMARY KEY CHECK (id = 1),
        file        TEXT NOT NULL,
        imported_at INTEGER NOT NULL,
        xml_tracks  INTEGER NOT NULL,
        matched     INTEGER NOT NULL
    );
    CREATE TABLE rekordbox_reload (
        track_id   INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        changed_at INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE path_journal (
        id           INTEGER PRIMARY KEY,
        track_id     INTEGER,
        old_path     TEXT NOT NULL,
        new_path     TEXT NOT NULL,
        in_rekordbox INTEGER NOT NULL DEFAULT 0,
        at           INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE playlists (
        id         INTEGER PRIMARY KEY,
        name       TEXT NOT NULL,
        kind       TEXT NOT NULL DEFAULT 'static',
        rules      TEXT,
        created_at INTEGER NOT NULL DEFAULT (unixepoch())
    );
    CREATE TABLE playlist_tracks (
        playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        position    INTEGER NOT NULL,
        PRIMARY KEY (playlist_id, track_id)
    );",
    // 8 — MP3 : le tag ID3v2 est désormais sauté avant le décodage (docs/DECISIONS.md, D-413).
    // Quelques MP3 étaient décodés de travers (pochette prise pour du son) : leurs empreintes,
    // formes d'onde et analyses de qualité sont recalculées. Aucune donnée de l'utilisateur
    // n'est perdue (ce sont des calculs refaits automatiquement ou à la demande).
    "UPDATE tracks SET audio_hash = NULL WHERE format = 'mp3';
    DELETE FROM fingerprints WHERE track_id IN (SELECT id FROM tracks WHERE format = 'mp3');
    DELETE FROM waveforms WHERE track_id IN (SELECT id FROM tracks WHERE format = 'mp3');
    DELETE FROM quality WHERE track_id IN (SELECT id FROM tracks WHERE format = 'mp3');",
    // 9 — Relecture de tous les fichiers au prochain scan, même inchangés : les titres
    // « Artiste - Titre » sont désormais séparés à la lecture (docs/DECISIONS.md, D-414).
    // Les champs verrouillés ne sont jamais remplacés par une relecture.
    "ALTER TABLE tracks ADD COLUMN reread INTEGER NOT NULL DEFAULT 0;
    UPDATE tracks SET reread = 1;",
];

/// Ouvre une connexion supplémentaire (tâches de fond). La base doit déjà exister et être migrée.
pub fn open_worker(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

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

// ---------- Sauvegardes, export, import ----------

const KEEP_BACKUPS: usize = 7;
const IMPORT_FILE: &str = "library.import.sqlite";

/// Date du jour (AAAA-MM-JJ, temps universel) sans dépendance supplémentaire.
pub fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Algorithme de conversion jours → date civile (H. Hinnant).
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

/// Sauvegarde quotidienne de la base (7 dernières copies conservées).
pub fn daily_backup(conn: &Connection, data_dir: &Path) -> Result<Option<PathBuf>, AppError> {
    let dir = data_dir.join("backups");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("library-{}.sqlite", today()));
    if target.exists() {
        return Ok(None);
    }
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;
    let mut copies: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with("library-2") && n.ends_with(".sqlite")).unwrap_or(false))
        .collect();
    copies.sort();
    while copies.len() > KEEP_BACKUPS {
        let old = copies.remove(0);
        let _ = std::fs::remove_file(old);
    }
    log::info!("Sauvegarde de la base : {}", target.display());
    Ok(Some(target))
}

/// Exporte la base dans un fichier choisi par l'utilisateur.
pub fn export(conn: &Connection, dest: &Path) -> Result<(), AppError> {
    if dest.exists() {
        // L'utilisateur a confirmé le remplacement dans la fenêtre d'enregistrement.
        std::fs::remove_file(dest)?;
    }
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
    Ok(())
}

/// Prépare l'import d'une base : vérifiée maintenant, mise en place au prochain démarrage.
pub fn stage_import(data_dir: &Path, src: &Path) -> Result<(), AppError> {
    let conn = Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let v = current_version(&conn)?;
    if v > latest_version() {
        return Err(AppError::DatabaseTooNew { found: v, supported: latest_version() });
    }
    let ok: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('settings', 'tracks')",
        [],
        |r| r.get(0),
    )?;
    if v < 1 || ok < 1 {
        return Err(AppError::InvalidSetting("ce fichier n'est pas une base de l'application".into()));
    }
    drop(conn);
    std::fs::copy(src, data_dir.join(IMPORT_FILE))?;
    Ok(())
}

/// Au démarrage : une base importée remplace l'actuelle, qui est d'abord sauvegardée.
fn apply_pending_import(data_dir: &Path, path: &Path) -> Result<(), AppError> {
    let pending = data_dir.join(IMPORT_FILE);
    if !pending.exists() {
        return Ok(());
    }
    if path.exists() {
        let dir = data_dir.join("backups");
        std::fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let conn = Connection::open(path)?;
        conn.execute("VACUUM INTO ?1", [dir.join(format!("avant-import-{stamp}.sqlite")).to_string_lossy().as_ref()])?;
        drop(conn);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(PathBuf::from(format!("{}{suffix}", path.display())));
        }
    }
    std::fs::rename(&pending, path)?;
    log::info!("Base importée mise en place");
    Ok(())
}

/// Ouvre (ou crée) la base dans le dossier de données de l'application.
/// Avant de migrer une base existante, une copie de sécurité est faite à côté.
pub fn open(data_dir: &Path) -> Result<(Connection, PathBuf), AppError> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join("library.sqlite");
    apply_pending_import(data_dir, &path)?;
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
    fn date_du_jour_valide() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }

    #[test]
    fn sauvegardes_export_import() {
        let dir = tempfile::tempdir().unwrap();
        let (conn, path) = open(dir.path()).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('theme', 'dark')", []).unwrap();
        assert!(daily_backup(&conn, dir.path()).unwrap().is_some());
        assert!(daily_backup(&conn, dir.path()).unwrap().is_none(), "une seule sauvegarde par jour");
        // Garder 7 copies au plus
        for i in 0..10 {
            std::fs::write(dir.path().join("backups").join(format!("library-2000-01-{i:02}.sqlite")), b"x").unwrap();
        }
        std::fs::remove_file(dir.path().join("backups").join(format!("library-{}.sqlite", today()))).unwrap();
        daily_backup(&conn, dir.path()).unwrap();
        let n = std::fs::read_dir(dir.path().join("backups")).unwrap().count();
        assert_eq!(n, 7);

        let exported = dir.path().join("export.sqlite");
        export(&conn, &exported).unwrap();
        conn.execute("UPDATE settings SET value = 'light' WHERE key = 'theme'", []).unwrap();
        drop(conn);
        stage_import(dir.path(), &exported).unwrap();
        assert!(stage_import(dir.path(), &dir.path().join("backups").join("library-2000-01-09.sqlite")).is_err(), "faux fichier accepté");
        let (conn, _) = open(dir.path()).unwrap();
        let v: String = conn.query_row("SELECT value FROM settings WHERE key = 'theme'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "dark", "la base importée n'a pas été mise en place");
        assert!(std::fs::read_dir(dir.path().join("backups")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("avant-import")));
        let _ = path;
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
