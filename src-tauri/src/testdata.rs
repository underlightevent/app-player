//! Fichiers audio de test, fabriqués à la volée (son synthétique, libre de droits).
//! WAV, AIFF et FLAC contiennent une vraie onde ; le MP3 est une suite de trames
//! valides de silence (pas d'encodeur MP3 libre de droits en Rust).

#![cfg(test)]

use std::path::{Path, PathBuf};

pub const RATE: u32 = 44_100;

/// Échantillons stéréo 16 bits : une onde différente selon `seed`.
pub fn samples(frames: usize, seed: u32) -> Vec<i16> {
    let mut out = Vec::with_capacity(frames * 2);
    let freq = 220.0 + seed as f64 * 37.0;
    for i in 0..frames {
        let t = i as f64 / RATE as f64;
        let v = (t * freq * std::f64::consts::TAU).sin() * 9000.0;
        out.push(v as i16);
        out.push((v * 0.5) as i16);
    }
    out
}

pub fn wav_bytes(seed: u32) -> Vec<u8> {
    let s = samples(RATE as usize / 2, seed);
    let data: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut out = b"RIFF".to_vec();
    out.extend(((4 + 8 + 16 + 8 + data.len()) as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(2u16.to_le_bytes());
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 4).to_le_bytes());
    out.extend(4u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}

pub fn aiff_bytes(seed: u32) -> Vec<u8> {
    let s = samples(RATE as usize / 2, seed);
    let frames = (s.len() / 2) as u32;
    let data: Vec<u8> = s.iter().flat_map(|v| v.to_be_bytes()).collect();
    let mut comm = Vec::new();
    comm.extend(2u16.to_be_bytes());
    comm.extend(frames.to_be_bytes());
    comm.extend(16u16.to_be_bytes());
    comm.extend([0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]); // 44100 en flottant 80 bits
    let mut body = b"AIFF".to_vec();
    body.extend(b"COMM");
    body.extend((comm.len() as u32).to_be_bytes());
    body.extend(comm);
    body.extend(b"SSND");
    body.extend(((data.len() + 8) as u32).to_be_bytes());
    body.extend(0u32.to_be_bytes());
    body.extend(0u32.to_be_bytes());
    body.extend(data);
    let mut out = b"FORM".to_vec();
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for b in data {
        crc ^= (*b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x8005 } else { crc << 1 };
        }
    }
    crc
}

/// FLAC minimal : trames « VERBATIM » (sans compression), parfaitement valides.
pub fn flac_bytes(seed: u32) -> Vec<u8> {
    let s = samples(RATE as usize / 2, seed);
    let frames = s.len() / 2;
    let block = 256usize;
    let mut out = b"fLaC".to_vec();
    // STREAMINFO (dernier bloc de métadonnées)
    out.push(0x80);
    out.extend(&34u32.to_be_bytes()[1..]);
    let mut si = Vec::new();
    si.extend((block as u16).to_be_bytes());
    si.extend((block as u16).to_be_bytes());
    si.extend([0u8; 6]); // tailles de trame inconnues
    let total = frames as u64;
    // 20 bits fréquence, 3 bits canaux-1, 5 bits bits-1, 36 bits nombre d'échantillons
    let packed: u64 = ((RATE as u64) << 44) | (1u64 << 41) | (15u64 << 36) | total;
    si.extend(packed.to_be_bytes());
    si.extend([0u8; 16]); // MD5 inconnu
    out.extend(si);
    for (n, chunk) in s.chunks(block * 2).enumerate() {
        let len = chunk.len() / 2;
        // 0x6 : taille de bloc sur 8 bits en fin d'en-tête ; 0x9 : 44,1 kHz
        let mut f = vec![0xFF, 0xF8, 0x60 | 0x09, (1 << 4) | (0b100 << 1)];
        assert!(n < 128);
        f.push(n as u8);
        f.push((len - 1) as u8);
        let c = crc8(&f);
        f.push(c);
        for ch in 0..2 {
            f.push(0x02); // sous-trame VERBATIM
            for i in 0..len {
                f.extend(chunk[i * 2 + ch].to_be_bytes());
            }
        }
        let c = crc16(&f);
        f.extend(c.to_be_bytes());
        out.extend(f);
    }
    out
}

/// MP3 : 40 trames MPEG-1 couche III, 128 kbit/s, 44,1 kHz, mono, silence.
pub fn mp3_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..40 {
        let mut frame = vec![0xFF, 0xFB, 0x90, 0xC0];
        frame.resize(417, 0);
        out.extend(frame);
    }
    out
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// Un fichier de chaque format prioritaire.
pub fn all_formats(dir: &Path) -> Vec<PathBuf> {
    vec![
        write(dir, "test.wav", &wav_bytes(1)),
        write(dir, "test.mp3", &mp3_bytes()),
        write(dir, "test.flac", &flac_bytes(2)),
        write(dir, "test.aiff", &aiff_bytes(3)),
    ]
}

/// Bibliothèque de démonstration pour les essais manuels :
/// `DJLM_DEMO_DIR=C:\chemin cargo test generer_bibliotheque_de_demo -- --ignored`
#[test]
#[ignore]
fn generer_bibliotheque_de_demo() {
    use lofty::config::WriteOptions;
    use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};
    let dir = PathBuf::from(std::env::var("DJLM_DEMO_DIR").expect("DJLM_DEMO_DIR"));
    let count: usize = std::env::var("DJLM_DEMO_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let genres = ["Techno", "House", "Trance", "Drum & Bass", "Disco"];
    let versions = ["Extended Mix", "Original Mix", "Radio Edit", "DJ Lumière Remix", ""];
    for i in 0..count {
        let sub = dir.join(genres[i % genres.len()]);
        std::fs::create_dir_all(&sub).unwrap();
        let (ext, bytes, tag_type) = match i % 4 {
            0 => ("wav", wav_bytes(i as u32 % 20), TagType::Id3v2),
            1 => ("mp3", mp3_bytes(), TagType::Id3v2),
            2 => ("flac", flac_bytes(i as u32 % 20), TagType::VorbisComments),
            _ => ("aiff", aiff_bytes(i as u32 % 20), TagType::Id3v2),
        };
        let path = sub.join(format!("piste_{i:04}.{ext}"));
        std::fs::write(&path, bytes).unwrap();
        if i % 10 == 9 {
            continue; // quelques fichiers sans aucun tag
        }
        let mut tag = Tag::new(tag_type);
        let v = versions[i % versions.len()];
        let title = format!("Morceau n°{i} Été");
        tag.set_title(if v.is_empty() { title } else { format!("{title} ({v})") });
        tag.set_artist(if i % 3 == 0 { format!("Artiste {} feat. Zoë", i % 37) } else { format!("Artiste {}", i % 37) });
        tag.set_album(format!("Album {}", i % 50));
        tag.set_genre(genres[i % genres.len()].to_string());
        tag.insert_text(ItemKey::Year, format!("{}", 2000 + i % 25));
        tag.insert_text(ItemKey::IntegerBpm, format!("{}", 118 + i % 20));
        tag.insert_text(ItemKey::InitialKey, ["Am", "8A", "F#m", "Db"][i % 4].to_string());
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
    }
    std::fs::write(dir.join("fichier_casse.mp3"), b"pas un mp3").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::hash::audio_stream_hash;
    use crate::tags::read::read_file;

    #[test]
    fn fichiers_de_test_lisibles() {
        let dir = tempfile::tempdir().unwrap();
        for p in all_formats(dir.path()) {
            let info = read_file(&p, &[]).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(info.duration_ms > 0, "{}: durée nulle", p.display());
            assert!(info.sample_rate == Some(RATE), "{}: {:?}", p.display(), info.sample_rate);
            let h = audio_stream_hash(&p, None).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(h.len(), 64);
        }
    }

    #[test]
    fn hash_different_selon_le_son() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.wav", &wav_bytes(1));
        let b = write(dir.path(), "b.wav", &wav_bytes(2));
        assert_ne!(audio_stream_hash(&a, None).unwrap(), audio_stream_hash(&b, None).unwrap());
    }
}
