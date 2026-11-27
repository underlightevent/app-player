//! Réparation des dates ID3 abîmées (TDRC, TDEN… mal formées).
//!
//! Ces trames rendent le tag illisible en mode strict : l'application refuse alors d'écrire
//! les tags du fichier, par sécurité. La réparation remplace **seulement** les trames de date
//! invalides par du remplissage (des zéros), sans changer la taille du tag ni du fichier :
//! le son et toutes les autres trames restent octet pour octet identiques. Les octets
//! d'origine du tag sont gardés dans l'historique : Ctrl+Z les remet exactement.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::library::edit::{EditFailure, EditReport};
use crate::library::{index, volumes, Library};
use crate::tags::write::CoverData;

const DATE_FRAMES: &[&str] = &["TDRC", "TDEN", "TDOR", "TDRL", "TDTG", "TYER", "TDAT", "TIME", "TRDA", "TORY"];

/// Emplacement du tag ID3v2 dans le fichier (en-tête compris).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub offset: u64,
    pub len: usize,
}

fn syncsafe(b: &[u8]) -> usize {
    b.iter().take(4).fold(0usize, |a, x| (a << 7) | (*x as usize & 0x7f))
}

/// Tag ID3v2 au début (MP3) ou dans le bloc « ID3 » (WAV, AIFF).
pub fn find_region(path: &Path) -> std::io::Result<Option<Region>> {
    let mut f = File::open(path)?;
    let mut head = [0u8; 10];
    if f.read(&mut head)? < 10 {
        return Ok(None);
    }
    let start = if &head[..3] == b"ID3" {
        0
    } else if &head[..4] == b"RIFF" || &head[..4] == b"FORM" {
        let layout = crate::tags::chunks::read_layout(&mut f)?;
        match layout.chunks.iter().find(|c| c.is_id3()) {
            Some(c) => c.data_offset,
            None => return Ok(None),
        }
    } else {
        return Ok(None);
    };
    f.seek(SeekFrom::Start(start))?;
    f.read_exact(&mut head)?;
    if &head[..3] != b"ID3" {
        return Ok(None);
    }
    Ok(Some(Region { offset: start, len: 10 + syncsafe(&head[6..10]) }))
}

fn decode_text(content: &[u8]) -> Option<String> {
    let (&enc, rest) = content.split_first()?;
    let s = match enc {
        0 => rest.iter().map(|b| *b as char).collect(),
        3 => String::from_utf8(rest.to_vec()).ok()?,
        1 | 2 => {
            let mut bytes = rest;
            let mut le = enc == 1;
            if bytes.len() >= 2 && enc == 1 {
                le = bytes[..2] == [0xFF, 0xFE];
                if bytes[..2] == [0xFF, 0xFE] || bytes[..2] == [0xFE, 0xFF] {
                    bytes = &bytes[2..];
                }
            }
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
                .collect();
            String::from_utf16(&units).ok()?
        }
        _ => return None,
    };
    Some(s.trim_matches(char::from(0)).trim().to_string())
}

/// Une date ID3 est-elle bien formée ? (AAAA, AAAA-MM, AAAA-MM-JJ, AAAA-MM-JJTHH:MM:SS…)
pub fn valid_date(id: &str, content: &[u8]) -> bool {
    let Some(text) = decode_text(content) else { return false };
    // Plusieurs valeurs séparées par un zéro (ID3v2.4) : chacune doit être valide.
    text.split('\0').all(|v| {
        let v = v.trim();
        let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
        match id {
            "TYER" | "TORY" => digits(v, 4),
            "TDAT" | "TIME" => digits(v, 4),
            "TRDA" => !v.is_empty(),
            _ => {
                // AAAA[-MM[-JJ[THH[:MM[:SS]]]]]
                let b = v.as_bytes();
                let ok_len = [4usize, 7, 10, 13, 16, 19].contains(&b.len());
                ok_len
                    && b.iter().enumerate().all(|(i, c)| match i {
                        4 | 7 => *c == b'-',
                        10 => *c == b'T',
                        13 | 16 => *c == b':',
                        _ => c.is_ascii_digit(),
                    })
            }
        }
    })
}

/// Nouveau tag (même longueur) sans les trames de date invalides, et leurs identifiants.
/// `None` : rien à réparer, ou tag d'une forme qu'on ne touche pas (désynchronisation,
/// en-tête étendu, version inconnue).
pub fn repair_bytes(tag: &[u8]) -> Option<(Vec<u8>, Vec<String>)> {
    if tag.len() < 10 || &tag[..3] != b"ID3" {
        return None;
    }
    let version = tag[3];
    let flags = tag[5];
    if !(version == 3 || version == 4) || flags & 0x80 != 0 || flags & 0x40 != 0 {
        return None;
    }
    let mut kept: Vec<u8> = tag[..10].to_vec();
    let mut removed = Vec::new();
    let mut pos = 10;
    while pos + 10 <= tag.len() {
        let id = &tag[pos..pos + 4];
        if id[0] == 0 {
            break; // remplissage
        }
        if !id.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return None; // structure inattendue : on ne touche à rien
        }
        let size = if version == 4 { syncsafe(&tag[pos + 4..pos + 8]) } else { u32::from_be_bytes([tag[pos + 4], tag[pos + 5], tag[pos + 6], tag[pos + 7]]) as usize };
        let fflags = tag[pos + 9];
        let end = pos + 10 + size;
        if end > tag.len() {
            return None;
        }
        let id_s = std::str::from_utf8(id).unwrap_or("");
        // Trames compressées ou chiffrées : contenu non lisible directement, gardé tel quel.
        let plain = fflags & 0x0f == 0 && (version == 4 || fflags & 0xc0 == 0);
        if DATE_FRAMES.contains(&id_s) && plain && !valid_date(id_s, &tag[pos + 10..end]) {
            removed.push(id_s.to_string());
        } else {
            kept.extend_from_slice(&tag[pos..end]);
        }
        pos = end;
    }
    if removed.is_empty() {
        return None;
    }
    kept.resize(tag.len(), 0);
    Some((kept, removed))
}

fn read_region(path: &Path, r: Region) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(r.offset))?;
    let mut buf = vec![0u8; r.len];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// Le tag se relit-il en mode strict (celui de l'écriture des tags) ?
pub fn strict_ok(path: &Path) -> bool {
    use lofty::config::{ParseOptions, ParsingMode};
    let opts = ParseOptions::new().parsing_mode(ParsingMode::Strict).read_properties(false);
    let Ok(mut f) = File::open(path) else { return false };
    match lofty::probe::Probe::new(&mut f).options(opts).guess_file_type() {
        Ok(p) => p.read().is_ok(),
        Err(_) => false,
    }
}

/// Écrit `bytes` à la place du tag, sur une copie vérifiée (son identique, tag lisible),
/// puis remplace l'original.
fn write_region(path: &Path, r: Region, expect_old: &[u8], bytes: &[u8], require_strict: bool) -> Result<(), String> {
    if bytes.len() != r.len {
        return Err("longueur du tag différente".into());
    }
    let current = read_region(path, r).map_err(|e| e.to_string())?;
    if current != expect_old {
        return Err("le fichier a changé depuis".into());
    }
    let tmp = path.with_file_name(format!(".djlm-dates-{}.tmp", std::process::id()));
    let res = (|| -> Result<(), String> {
        std::fs::copy(path, &tmp).map_err(|e| e.to_string())?;
        {
            let mut f = OpenOptions::new().write(true).open(&tmp).map_err(|e| e.to_string())?;
            f.seek(SeekFrom::Start(r.offset)).map_err(|e| e.to_string())?;
            f.write_all(bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        if require_strict && !strict_ok(&tmp) {
            return Err("le tag reste illisible après réparation (autre défaut)".into());
        }
        let before = crate::audio::hash::audio_stream_hash(path, None).map_err(|e| e.to_string())?;
        let after = crate::audio::hash::audio_stream_hash(&tmp, None).map_err(|e| e.to_string())?;
        if before != after {
            return Err("le son aurait changé".into());
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairItem {
    pub track_id: i64,
    pub file_name: String,
    pub frames: Vec<String>,
}

/// État enregistré dans l'historique (octets complets du tag avant / après).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTagState {
    pub volume_id: i64,
    pub rel_path: String,
    pub offset: u64,
    pub len: usize,
    /// Empreinte du blob (table des pochettes de l'historique).
    pub blob: String,
    /// Octets attendus actuellement (pour refuser si le fichier a changé).
    pub expect: String,
}

fn track_path(lib: &Library, conn: &Connection, id: i64) -> Result<Option<(i64, Option<i64>, String, String, std::path::PathBuf)>, AppError> {
    use rusqlite::OptionalExtension;
    let row: Option<(i64, Option<i64>, String, String)> = conn
        .query_row("SELECT volume_id, root_id, rel_path, file_name FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?;
    let Some((vol, root, rel, name)) = row else { return Ok(None) };
    let Some(mount) = lib.mounts().get(&vol).cloned() else { return Ok(None) };
    let p = volumes::join_rel(&mount, &rel);
    Ok(Some((vol, root, rel, name, p)))
}

/// Aperçu : morceaux dont des dates ID3 sont abîmées, et lesquelles.
pub fn plan(lib: &Library, conn: &Connection, ids: &[i64]) -> Result<Vec<RepairItem>, AppError> {
    let mut out = Vec::new();
    for &id in ids {
        let Some((_, _, _, name, path)) = track_path(lib, conn, id)? else { continue };
        let Ok(Some(r)) = find_region(&path) else { continue };
        let Ok(tag) = read_region(&path, r) else { continue };
        if let Some((_, frames)) = repair_bytes(&tag) {
            out.push(RepairItem { track_id: id, file_name: name, frames });
        }
    }
    Ok(out)
}

fn reindex(conn: &Connection, vol: i64, root: Option<i64>, rel: &str, path: &Path) -> Result<(), AppError> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let exceptions = index::artist_exceptions(conn).unwrap_or_default();
    index::store(conn, vol, root, rel, meta.len() as i64, mtime, crate::tags::read::read_file(path, &exceptions))?;
    Ok(())
}

fn fail(id: i64, name: &str, code: &str, message: String) -> EditFailure {
    EditFailure { track_id: id, file_name: name.to_string(), code: code.into(), message }
}

/// Répare les morceaux `ids` (un seul lot d'historique : Ctrl+Z remet les tags d'origine).
pub fn apply(lib: &Library, conn: &Connection, ids: &[i64]) -> Result<EditReport, AppError> {
    let batch = crate::history::begin_batch(conn, "history.repairDates", None)?;
    let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
    for &id in ids {
        let Some((vol, root, rel, name, path)) = track_path(lib, conn, id)? else {
            report.failures.push(fail(id, "", "offline", "disque déconnecté".into()));
            continue;
        };
        let res = (|| -> Result<bool, String> {
            let Some(r) = find_region(&path).map_err(|e| e.to_string())? else { return Ok(false) };
            let old = read_region(&path, r).map_err(|e| e.to_string())?;
            let Some((new, _)) = repair_bytes(&old) else { return Ok(false) };
            write_region(&path, r, &old, &new, true)?;
            let put = |b: &[u8]| crate::history::put_blob(conn, &CoverData { mime: "application/octet-stream".into(), data: b.to_vec() }).map_err(|e| e.to_string());
            let (ob, nb) = (put(&old)?, put(&new)?);
            let before = RawTagState { volume_id: vol, rel_path: rel.clone(), offset: r.offset, len: r.len, blob: ob.clone(), expect: nb.clone() };
            let after = RawTagState { volume_id: vol, rel_path: rel.clone(), offset: r.offset, len: r.len, blob: nb, expect: ob };
            crate::history::record(conn, batch, "raw_tag", Some(id), &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())
                .map_err(|e| e.to_string())?;
            reindex(conn, vol, root, &rel, &path).map_err(|e| e.to_string())?;
            Ok(true)
        })();
        match res {
            Ok(true) => report.ok += 1,
            Ok(false) => {}
            Err(e) => report.failures.push(fail(id, &name, "repair", e)),
        }
    }
    if crate::history::drop_if_empty(conn, batch)? {
        report.batch_id = None;
    }
    lib.notify_changed(true);
    Ok(report)
}

/// Annulation / rétablissement (historique « raw_tag »).
pub fn restore(lib: &Library, conn: &Connection, id: i64, state_json: &str) -> Result<(), EditFailure> {
    let st: RawTagState = serde_json::from_str(state_json).map_err(|e| fail(id, "", "internal", e.to_string()))?;
    let Some(mount) = lib.mounts().get(&st.volume_id).cloned() else { return Err(fail(id, "", "offline", "disque déconnecté".into())) };
    let path = volumes::join_rel(&mount, &st.rel_path);
    let blob = |h: &str| crate::history::get_blob(conn, h).ok().flatten().map(|c| c.data);
    let (Some(bytes), Some(expect)) = (blob(&st.blob), blob(&st.expect)) else { return Err(fail(id, "", "internal", "historique incomplet".into())) };
    write_region(&path, Region { offset: st.offset, len: st.len }, &expect, &bytes, false).map_err(|e| fail(id, &st.rel_path, "repair", e))?;
    let root: Option<i64> = conn.query_row("SELECT root_id FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap_or(None);
    reindex(conn, st.volume_id, root, &st.rel_path, &path).map_err(|e| fail(id, &st.rel_path, "internal", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_v3(id: &str, text: &str) -> Vec<u8> {
        let mut c = vec![0u8];
        c.extend_from_slice(text.as_bytes());
        let mut f = id.as_bytes().to_vec();
        f.extend_from_slice(&(c.len() as u32).to_be_bytes());
        f.extend_from_slice(&[0, 0]);
        f.extend_from_slice(&c);
        f
    }

    fn tag_v3(frames: &[Vec<u8>], padding: usize) -> Vec<u8> {
        let body: Vec<u8> = frames.concat();
        let n = body.len() + padding;
        let mut t = b"ID3\x03\x00\x00".to_vec();
        t.extend_from_slice(&[(n >> 21) as u8 & 0x7f, (n >> 14) as u8 & 0x7f, (n >> 7) as u8 & 0x7f, n as u8 & 0x7f]);
        t.extend_from_slice(&body);
        t.resize(10 + n, 0);
        t
    }

    #[test]
    fn dates_valides_ou_non() {
        assert!(valid_date("TDRC", b"\x002024"));
        assert!(valid_date("TDRC", b"\x002024-05-01"));
        assert!(valid_date("TDRC", b"\x002024-05-01T12:30"));
        assert!(!valid_date("TDRC", b"\x0020"));
        assert!(!valid_date("TDRC", b"\x002024-"));
        assert!(!valid_date("TDEN", b"\x00hier"));
        assert!(valid_date("TYER", b"\x001999"));
        assert!(!valid_date("TYER", b"\x0099"));
    }

    #[test]
    fn reparation_garde_la_longueur_et_le_reste() {
        let title = frame_v3("TIT2", "Clown");
        let bad = frame_v3("TDEN", "2019-");
        let good = frame_v3("TYER", "2019");
        let tag = tag_v3(&[title.clone(), bad, good.clone()], 32);
        let (new, removed) = repair_bytes(&tag).unwrap();
        assert_eq!(removed, ["TDEN"]);
        assert_eq!(new.len(), tag.len(), "même taille");
        assert_eq!(&new[10..10 + title.len()], &title[..], "TIT2 intact");
        assert_eq!(&new[10 + title.len()..10 + title.len() + good.len()], &good[..], "TYER valide gardé");
        assert!(repair_bytes(&new).is_none(), "plus rien à réparer");
        assert!(repair_bytes(&tag_v3(&[title], 16)).is_none());
    }

    #[test]
    fn mp3_repare_puis_annule() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        let mut bytes = tag_v3(&[frame_v3("TIT2", "Clown"), frame_v3("TPE1", "Soprano"), frame_v3("TDEN", "2019-")], 64);
        bytes.extend_from_slice(&crate::testdata::mp3_bytes());
        let p = crate::testdata::write(&music, "clown.mp3", &bytes);
        assert!(!strict_ok(&p), "le cas doit reproduire le refus du mode strict");
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let plan = plan(&lib, &conn, &ids).unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].frames, ["TDEN"]);
        let hash = crate::audio::hash::audio_stream_hash(&p, None).unwrap();
        let r = apply(&lib, &conn, &ids).unwrap();
        assert_eq!(r.ok, 1, "{:?}", r.failures);
        assert!(strict_ok(&p));
        assert_eq!(std::fs::metadata(&p).unwrap().len(), bytes.len() as u64, "même taille de fichier");
        assert_eq!(crate::audio::hash::audio_stream_hash(&p, None).unwrap(), hash, "son identique");
        // Annulation : octets d'origine.
        let u = lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert_eq!(u.ok, 1, "{:?}", u.failures);
        assert_eq!(std::fs::read(&p).unwrap(), bytes, "fichier d'origine, octet pour octet");
    }
}
