//! Forme d'onde : 1 000 colonnes (crête de chaque tranche, 0 à 255), calculée une fois
//! puis gardée en cache dans la base.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::decode::Decoder;
use crate::error::AppError;

pub const BINS: usize = 1000;

/// Calcule la forme d'onde d'un fichier (décodage complet).
pub fn compute(path: &Path) -> Result<Vec<u8>, String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels as usize;
    // Crête par bloc de 512 trames, puis regroupement en 1 000 colonnes.
    let mut peaks: Vec<f32> = Vec::new();
    let mut block_peak = 0f32;
    let mut in_block = 0usize;
    let mut buf = Vec::new();
    while dec.next_block(&mut buf)? {
        for frame in buf.chunks(ch) {
            let v = frame.iter().fold(0f32, |m, s| m.max(s.abs()));
            block_peak = block_peak.max(v);
            in_block += 1;
            if in_block == 512 {
                peaks.push(block_peak);
                block_peak = 0.0;
                in_block = 0;
            }
        }
    }
    if in_block > 0 {
        peaks.push(block_peak);
    }
    if peaks.is_empty() {
        return Ok(vec![0; BINS]);
    }
    let max = peaks.iter().cloned().fold(0f32, f32::max).max(1e-6);
    let mut out = Vec::with_capacity(BINS);
    for i in 0..BINS {
        let a = i * peaks.len() / BINS;
        let b = ((i + 1) * peaks.len() / BINS).max(a + 1).min(peaks.len());
        let p = peaks[a.min(peaks.len() - 1)..b].iter().cloned().fold(0f32, f32::max);
        out.push(((p / max).sqrt() * 255.0).round() as u8); // racine : les passages calmes restent visibles
    }
    Ok(out)
}

/// Forme d'onde en cache, si elle correspond toujours au fichier (taille et date).
pub fn cached(conn: &Connection, track_id: i64) -> Result<Option<Vec<u8>>, AppError> {
    Ok(conn
        .query_row(
            "SELECT w.data FROM waveforms w JOIN tracks t ON t.id = w.track_id
             WHERE w.track_id = ?1 AND w.size = t.size AND w.mtime = t.mtime",
            [track_id],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn store(conn: &Connection, track_id: i64, data: &[u8]) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO waveforms (track_id, size, mtime, data)
         SELECT id, size, mtime, ?2 FROM tracks WHERE id = ?1
         ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, data = excluded.data",
        params![track_id, data],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forme_d_onde_de_chaque_format() {
        let dir = tempfile::tempdir().unwrap();
        for p in crate::testdata::all_formats(dir.path()) {
            let w = compute(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(w.len(), BINS);
            if !p.to_string_lossy().ends_with(".mp3") {
                // Le MP3 de test est du silence ; les autres contiennent une onde.
                assert!(w.iter().any(|v| *v > 100), "{}", p.display());
            }
        }
    }

    #[test]
    fn decodage_et_deplacement() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::testdata::write(dir.path(), "a.wav", &crate::testdata::wav_bytes(1));
        let mut d = Decoder::open(&p).unwrap();
        assert_eq!(d.sample_rate, 44_100);
        assert_eq!(d.channels, 2);
        assert!(d.duration_ms.unwrap() >= 490);
        d.seek_ms(250).unwrap();
        let mut buf = Vec::new();
        assert!(d.next_block(&mut buf).unwrap());
    }
}
