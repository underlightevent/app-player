//! Empreinte audio Chromaprint, calculée sur l'ordinateur (rusty-chromaprint, licence MIT).
//! Seule l'empreinte compressée et la durée sont envoyées à AcoustID, jamais le son.
//! Elle est calculée une fois, gardée en base, et resservira aux doublons (phase 3).

use std::path::Path;

use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};

use crate::audio::decode::Decoder;
use crate::error::AppError;

/// Comme l'outil officiel fpcalc : les 120 premières secondes suffisent.
const MAX_SECONDS: u64 = 120;

pub struct Fingerprint {
    /// Empreinte compressée au format attendu par AcoustID.
    pub encoded: String,
    pub raw: Vec<u32>,
    pub duration_s: u32,
}

pub fn compute(path: &Path) -> Result<Fingerprint, String> {
    let mut dec = Decoder::open(path)?;
    let config = Configuration::preset_test2();
    let mut fp = Fingerprinter::new(&config);
    fp.start(dec.sample_rate, dec.channels as u32).map_err(|e| format!("{e:?}"))?;
    let limit = MAX_SECONDS * dec.sample_rate as u64 * dec.channels as u64;
    let mut fed: u64 = 0;
    let mut total_frames: u64 = 0;
    let mut buf = Vec::new();
    let mut pcm: Vec<i16> = Vec::new();
    while dec.next_block(&mut buf)? {
        total_frames += (buf.len() / dec.channels as usize) as u64;
        if fed < limit {
            pcm.clear();
            pcm.extend(buf.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16));
            let take = ((limit - fed) as usize).min(pcm.len());
            fp.consume(&pcm[..take]);
            fed += take as u64;
        } else if dec.duration_ms.is_some() {
            break; // la durée est connue : inutile de tout décoder
        }
    }
    fp.finish();
    let raw = fp.fingerprint().to_vec();
    if raw.is_empty() {
        return Err("empreinte vide (morceau trop court ?)".into());
    }
    let compressed = FingerprintCompressor::from(&config).compress(&raw);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed);
    let duration_s = match dec.duration_ms {
        Some(ms) => (ms / 1000) as u32,
        None => (total_frames / dec.sample_rate.max(1) as u64) as u32,
    };
    Ok(Fingerprint { encoded, raw, duration_s })
}

/// Empreinte valide d'un son synthétique de 15 s (sert à tester les clés AcoustID
/// sans envoyer aucun morceau de l'utilisateur).
pub fn synthetic() -> Fingerprint {
    let config = Configuration::preset_test2();
    let mut fp = Fingerprinter::new(&config);
    let rate = 44_100u32;
    let _ = fp.start(rate, 2);
    let mut pcm = Vec::with_capacity(rate as usize * 2);
    for sec in 0..15u32 {
        pcm.clear();
        for i in 0..rate {
            let t = i as f64 / rate as f64;
            let f = 220.0 + (sec % 5) as f64 * 55.0;
            let v = ((t * f * std::f64::consts::TAU).sin() * 8000.0) as i16;
            pcm.push(v);
            pcm.push(v / 2);
        }
        fp.consume(&pcm);
    }
    fp.finish();
    let raw = fp.fingerprint().to_vec();
    let compressed = FingerprintCompressor::from(&config).compress(&raw);
    Fingerprint { encoded: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed), raw, duration_s: 15 }
}

fn raw_to_bytes(raw: &[u32]) -> Vec<u8> {
    raw.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn bytes_to_raw(b: &[u8]) -> Vec<u32> {
    b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Empreinte en cache (encore valable si le fichier n'a pas changé), sinon calcul et mise en cache.
pub fn get_or_compute(conn: &Connection, track_id: i64, path: &Path) -> Result<Fingerprint, String> {
    let cached: Option<(String, i64, Vec<u8>)> = conn
        .query_row(
            "SELECT f.fingerprint, f.duration_s, f.raw FROM fingerprints f JOIN tracks t ON t.id = f.track_id
             WHERE f.track_id = ?1 AND f.size = t.size AND f.mtime = t.mtime",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default())),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((encoded, d, raw)) = cached {
        return Ok(Fingerprint { encoded, raw: bytes_to_raw(&raw), duration_s: d as u32 });
    }
    let fp = compute(path)?;
    store(conn, track_id, &fp).map_err(|e| e.to_string())?;
    Ok(fp)
}

fn store(conn: &Connection, track_id: i64, fp: &Fingerprint) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO fingerprints (track_id, size, mtime, duration_s, fingerprint, raw)
         SELECT id, size, mtime, ?2, ?3, ?4 FROM tracks WHERE id = ?1
         ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime,
            duration_s = excluded.duration_s, fingerprint = excluded.fingerprint, raw = excluded.raw",
        params![track_id, fp.duration_s, fp.encoded, raw_to_bytes(&fp.raw)],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empreinte_stable_et_differente_selon_le_son() {
        let dir = tempfile::tempdir().unwrap();
        // Il faut quelques secondes de son : on assemble un WAV de 8 s.
        let make = |seed: u32| {
            let mut s = Vec::new();
            for k in 0..16 {
                s.extend(crate::testdata::samples(crate::testdata::RATE as usize / 2, seed + k % 3));
            }
            let data: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
            let mut out = b"RIFF".to_vec();
            out.extend(((36 + data.len()) as u32).to_le_bytes());
            out.extend(b"WAVEfmt ");
            out.extend(16u32.to_le_bytes());
            out.extend(1u16.to_le_bytes());
            out.extend(2u16.to_le_bytes());
            out.extend(44_100u32.to_le_bytes());
            out.extend((44_100u32 * 4).to_le_bytes());
            out.extend(4u16.to_le_bytes());
            out.extend(16u16.to_le_bytes());
            out.extend(b"data");
            out.extend((data.len() as u32).to_le_bytes());
            out.extend(data);
            out
        };
        let a = crate::testdata::write(dir.path(), "a.wav", &make(1));
        let b = crate::testdata::write(dir.path(), "b.wav", &make(7));
        let fa = compute(&a).unwrap();
        let fa2 = compute(&a).unwrap();
        let fb = compute(&b).unwrap();
        assert_eq!(fa.encoded, fa2.encoded);
        assert_ne!(fa.encoded, fb.encoded);
        assert_eq!(fa.duration_s, 8);
        assert!(!fa.encoded.contains('='), "base64 sans remplissage attendu");
    }
}
