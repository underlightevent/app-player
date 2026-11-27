//! Volume perçu (EBU R128) : volume intégré en LUFS et crête vraie en dBTP.
//! Mesure seulement : le son n'est jamais modifié. Résultat gardé par fichier (taille et date)
//! pour ne pas remesurer un fichier inchangé.

use std::path::Path;

use ebur128::{EbuR128, Mode};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::audio::decode::Decoder;
use crate::error::AppError;
use crate::library::{volumes, Library};
use crate::tasks::TaskHandle;

/// Au-dessus : « trop fort » ; en dessous : « trop faible » (masters de club : -10 à -6 LUFS).
pub const TOO_LOUD: f64 = -5.0;
pub const TOO_QUIET: f64 = -14.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loudness {
    pub lufs: f64,
    pub peak_db: f64,
}

/// Mesure d'un fichier.
pub fn measure(path: &Path) -> Result<Loudness, String> {
    let mut dec = Decoder::open(path)?;
    let channels = dec.channels.max(1) as u32;
    let mut meter = EbuR128::new(channels, dec.sample_rate, Mode::I | Mode::TRUE_PEAK).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    while dec.next_block(&mut buf)? {
        meter.add_frames_f32(&buf).map_err(|e| e.to_string())?;
    }
    let lufs = meter.loudness_global().map_err(|e| e.to_string())?;
    let mut peak: f64 = 0.0;
    for c in 0..channels {
        peak = peak.max(meter.true_peak(c).map_err(|e| e.to_string())?);
    }
    if !lufs.is_finite() {
        return Err("son trop court ou silencieux".into());
    }
    Ok(Loudness { lufs, peak_db: if peak > 0.0 { 20.0 * peak.log10() } else { -120.0 } })
}

/// Morceaux à mesurer : tous ceux sans mesure à jour (ou `ids`).
fn pending(conn: &Connection, ids: Option<&[i64]>) -> Result<Vec<(i64, i64, String, i64, i64)>, AppError> {
    let mut st = conn.prepare(
        "SELECT t.id, t.volume_id, t.rel_path, t.size, t.mtime FROM tracks t
         LEFT JOIN loudness l ON l.track_id = t.id
         WHERE t.status = 'ok' AND (l.track_id IS NULL OR l.size != t.size OR l.mtime != t.mtime)",
    )?;
    let rows: Vec<(i64, i64, String, i64, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    Ok(match ids {
        Some(ids) => rows.into_iter().filter(|r| ids.contains(&r.0)).collect(),
        None => rows,
    })
}

/// Mesure (tâche de fond annulable). Renvoie le nombre de morceaux mesurés.
pub fn run(lib: &Library, conn: &Connection, ids: Option<&[i64]>, task: Option<&TaskHandle>) -> Result<usize, AppError> {
    let todo = pending(conn, ids)?;
    if let Some(t) = task {
        t.set_total(todo.len() as u64);
    }
    let mounts = lib.mounts();
    let mut done = 0;
    for (i, (id, vol, rel, size, mtime)) in todo.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        if let Some(m) = mounts.get(vol) {
            let res = measure(&volumes::join_rel(m, rel));
            let (lufs, peak) = match res {
                Ok(l) => (Some(l.lufs), Some(l.peak_db)),
                Err(e) => {
                    log::info!("Volume non mesuré ({rel}) : {e}");
                    (None, None)
                }
            };
            conn.execute(
                "INSERT INTO loudness (track_id, size, mtime, lufs, peak_db) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, lufs = excluded.lufs,
                   peak_db = excluded.peak_db, analyzed_at = unixepoch()",
                params![id, size, mtime, lufs, peak],
            )?;
            done += 1;
        }
        if let Some(t) = task {
            t.progress(i as u64 + 1);
        }
        if i % 25 == 0 {
            lib.notify_changed(false);
        }
    }
    lib.notify_changed(true);
    Ok(done)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Loudness>, AppError> {
    Ok(conn
        .query_row("SELECT lufs, peak_db FROM loudness WHERE track_id = ?1 AND lufs IS NOT NULL", [id], |r| Ok(Loudness { lufs: r.get(0)?, peak_db: r.get(1)? }))
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sinus 1 kHz pleine échelle −20 dBFS stéréo : environ −20 LUFS… à ±0,5 LU près
    /// (la pondération K est presque plate à 1 kHz ; un sinus stéréo identique donne −20 + 3 − 3).
    #[test]
    fn sinus_de_reference() {
        let dir = tempfile::tempdir().unwrap();
        let rate = 44_100usize;
        let amp = 10f64.powf(-20.0 / 20.0) * 32767.0;
        let mut s: Vec<i16> = Vec::new();
        for i in 0..rate * 5 {
            let v = (amp * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate as f64).sin()) as i16;
            s.push(v);
            s.push(v);
        }
        let p = crate::testdata::write(dir.path(), "sinus.wav", &crate::testdata::wav_from_samples(&s));
        let l = measure(&p).unwrap();
        assert!((l.lufs - (-20.0)).abs() < 0.7, "{l:?}");
        assert!((l.peak_db - (-20.0)).abs() < 0.7, "{l:?}");
    }
}
