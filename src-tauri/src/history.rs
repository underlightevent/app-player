//! Historique persistant des opérations (principe 4 : tout s'annule, même après redémarrage).
//!
//! Une action de l'utilisateur forme un « lot » ; chaque fichier touché y ajoute une
//! opération avec son état avant et après. Les pochettes remplacées sont gardées à part
//! (table history_blobs) pour pouvoir être remises.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::tags::write::{Changes, CoverData};

/// État d'un morceau pour l'historique : valeurs des champs concernés, pochette, cadenas.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TagState {
    /// Valeurs des champs (la pochette est rangée à part).
    pub changes: Changes,
    /// Pochette : None = non concernée ; Some(None) = aucune ; Some(Some(hash)) = image gardée.
    pub cover: Option<Option<String>>,
    /// Champs verrouillés parmi les champs concernés.
    pub locked: Vec<String>,
}

/// État d'un fichier pour un renommage ou un déplacement : chemin relatif au disque.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathState {
    pub volume_id: i64,
    pub rel_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchInfo {
    pub id: i64,
    pub label: String,
    pub detail: Option<String>,
    pub created_at: i64,
    pub undone: bool,
    pub count: i64,
}

#[derive(Debug, Clone)]
pub struct OpRow {
    pub id: i64,
    pub kind: String,
    pub track_id: Option<i64>,
    pub before: String,
    pub after: String,
}

pub fn begin_batch(conn: &Connection, label: &str, detail: Option<&str>) -> Result<i64, AppError> {
    // Une nouvelle action efface la possibilité de rétablir les actions annulées.
    conn.execute("DELETE FROM history_batches WHERE undone = 1", [])?;
    conn.execute("INSERT INTO history_batches (label, detail) VALUES (?1, ?2)", params![label, detail])?;
    Ok(conn.last_insert_rowid())
}

pub fn record(conn: &Connection, batch: i64, kind: &str, track_id: Option<i64>, before: &str, after: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO history_ops (batch_id, kind, track_id, before, after) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![batch, kind, track_id, before, after],
    )?;
    Ok(())
}

/// Supprime un lot resté vide (aucun fichier n'a pu être modifié).
pub fn drop_if_empty(conn: &Connection, batch: i64) -> Result<bool, AppError> {
    let n: i64 = conn.query_row("SELECT count(*) FROM history_ops WHERE batch_id = ?1", [batch], |r| r.get(0))?;
    if n == 0 {
        conn.execute("DELETE FROM history_batches WHERE id = ?1", [batch])?;
        return Ok(true);
    }
    Ok(false)
}

pub fn put_blob(conn: &Connection, cover: &CoverData) -> Result<String, AppError> {
    let hash = blake3::hash(&cover.data).to_hex().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO history_blobs (hash, mime, data) VALUES (?1, ?2, ?3)",
        params![hash, cover.mime, cover.data],
    )?;
    Ok(hash)
}

pub fn get_blob(conn: &Connection, hash: &str) -> Result<Option<CoverData>, AppError> {
    Ok(conn
        .query_row("SELECT mime, data FROM history_blobs WHERE hash = ?1", [hash], |r| {
            Ok(CoverData { mime: r.get(0)?, data: r.get(1)? })
        })
        .optional()?)
}

pub fn list(conn: &Connection, limit: i64) -> Result<Vec<BatchInfo>, AppError> {
    let mut st = conn.prepare(
        "SELECT b.id, b.label, b.detail, b.created_at, b.undone, (SELECT count(*) FROM history_ops o WHERE o.batch_id = b.id)
         FROM history_batches b ORDER BY b.id DESC LIMIT ?1",
    )?;
    let rows = st
        .query_map([limit], |r| {
            Ok(BatchInfo {
                id: r.get(0)?,
                label: r.get(1)?,
                detail: r.get(2)?,
                created_at: r.get(3)?,
                undone: r.get(4)?,
                count: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn ops(conn: &Connection, batch: i64) -> Result<Vec<OpRow>, AppError> {
    let mut st = conn.prepare("SELECT id, kind, track_id, before, after FROM history_ops WHERE batch_id = ?1 ORDER BY id")?;
    let rows = st
        .query_map([batch], |r| Ok(OpRow { id: r.get(0)?, kind: r.get(1)?, track_id: r.get(2)?, before: r.get(3)?, after: r.get(4)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Dernier lot pouvant être annulé.
pub fn last_undoable(conn: &Connection) -> Result<Option<i64>, AppError> {
    Ok(conn.query_row("SELECT max(id) FROM history_batches WHERE undone = 0", [], |r| r.get(0))?)
}

/// Lot pouvant être rétabli : le plus ancien des lots annulés.
pub fn next_redoable(conn: &Connection) -> Result<Option<i64>, AppError> {
    Ok(conn.query_row("SELECT min(id) FROM history_batches WHERE undone = 1", [], |r| r.get(0))?)
}

pub fn set_undone(conn: &Connection, batch: i64, undone: bool) -> Result<(), AppError> {
    conn.execute("UPDATE history_batches SET undone = ?2 WHERE id = ?1", params![batch, undone])?;
    Ok(())
}

pub fn batch_label(conn: &Connection, batch: i64) -> Result<Option<(String, Option<String>)>, AppError> {
    Ok(conn
        .query_row("SELECT label, detail FROM history_batches WHERE id = ?1", [batch], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?)
}

/// Purge : on garde les 2 000 derniers lots et les pochettes encore utilisées.
pub fn prune(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM history_batches WHERE id NOT IN (SELECT id FROM history_batches ORDER BY id DESC LIMIT 2000)",
        [],
    )?;
    conn.execute(
        "DELETE FROM history_blobs WHERE NOT EXISTS (
            SELECT 1 FROM history_ops o WHERE instr(o.before, history_blobs.hash) > 0 OR instr(o.after, history_blobs.hash) > 0)",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn lots_annuler_retablir() {
        let conn = open_in_memory().unwrap();
        let a = begin_batch(&conn, "history.editTags", None).unwrap();
        record(&conn, a, "tags", Some(1), "{}", "{}").unwrap();
        let b = begin_batch(&conn, "history.editTags", None).unwrap();
        record(&conn, b, "tags", Some(1), "{}", "{}").unwrap();
        assert_eq!(last_undoable(&conn).unwrap(), Some(b));
        set_undone(&conn, b, true).unwrap();
        assert_eq!(last_undoable(&conn).unwrap(), Some(a));
        assert_eq!(next_redoable(&conn).unwrap(), Some(b));
        // Une nouvelle action efface le lot annulé.
        let c = begin_batch(&conn, "history.editTags", None).unwrap();
        assert_eq!(next_redoable(&conn).unwrap(), None);
        assert!(drop_if_empty(&conn, c).unwrap());
        assert_eq!(list(&conn, 10).unwrap().len(), 1);
    }

    #[test]
    fn pochettes_gardees() {
        let conn = open_in_memory().unwrap();
        let c = CoverData { mime: "image/jpeg".into(), data: vec![1, 2, 3] };
        let h = put_blob(&conn, &c).unwrap();
        assert_eq!(get_blob(&conn, &h).unwrap(), Some(c));
    }
}
