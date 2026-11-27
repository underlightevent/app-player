//! Conversion WAV → AIFF (option de la phase 4, lancée uniquement à la demande).
//!
//! Seule exception au principe « l'audio n'est jamais modifié » : le conteneur change,
//! les échantillons restent identiques au bit près (simple changement d'ordre des octets).
//! Garanties :
//! - l'AIFF est écrit dans un fichier temporaire, puis l'audio décodé des deux fichiers
//!   est comparé échantillon par échantillon ; au moindre écart, rien n'est gardé ;
//! - les tags (y compris ceux que l'application ne gère pas, comme les données Serato)
//!   et la pochette sont recopiés : Rekordbox sait les lire dans un AIFF ;
//! - le WAV d'origine part en quarantaine (restaurable), jamais à la poubelle ;
//! - tout s'annule en une fois (l'AIFF part alors en quarantaine et le WAV revient).

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::error::AppError;
use crate::history;
use crate::library::edit::{EditFailure, EditReport};
use crate::library::quarantine::QuarantineState;
use crate::library::{index, volumes, Library};
use crate::tags::chunks::{read_chunk, read_layout, Container};
use crate::tags::write::Changes;
use crate::tasks::TaskHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertItem {
    pub track_id: i64,
    pub file_name: String,
    pub target_name: String,
    /// ok, not_wav, unsupported, exists, offline
    pub status: String,
    pub detail: Option<String>,
    pub in_rekordbox: bool,
}

struct Pcm {
    channels: u16,
    rate: u32,
    bits: u16,
    data_offset: u64,
    data_len: u64,
}

/// Format du WAV : seul le PCM entier (8, 16, 24, 32 bits) se convertit sans perte en AIFF.
fn wav_pcm(path: &Path) -> Result<Pcm, String> {
    let mut f = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let layout = read_layout(&mut f).map_err(|e| e.to_string())?;
    if layout.container != Container::Wav || &layout.form_type != b"WAVE" {
        return Err("pas un WAV".into());
    }
    let fmt = layout.chunks.iter().find(|c| c.is(b"fmt ")).ok_or("bloc fmt absent")?;
    let data = layout.chunks.iter().find(|c| c.is(b"data")).ok_or("bloc data absent")?;
    let fb = read_chunk(&mut f, fmt).map_err(|e| e.to_string())?;
    if fb.len() < 16 {
        return Err("bloc fmt trop court".into());
    }
    let tag = u16::from_le_bytes([fb[0], fb[1]]);
    let channels = u16::from_le_bytes([fb[2], fb[3]]);
    let rate = u32::from_le_bytes([fb[4], fb[5], fb[6], fb[7]]);
    let bits = u16::from_le_bytes([fb[14], fb[15]]);
    let pcm = match tag {
        1 => true,
        0xFFFE => fb.len() >= 26 && u16::from_le_bytes([fb[24], fb[25]]) == 1, // WAVE_FORMAT_EXTENSIBLE, sous-format PCM
        _ => false,
    };
    if !pcm {
        return Err(if tag == 3 { "float".into() } else { format!("codage {tag:#x}") });
    }
    if ![8, 16, 24, 32].contains(&bits) || channels == 0 {
        return Err(format!("{bits} bits"));
    }
    Ok(Pcm { channels, rate, bits, data_offset: data.data_offset, data_len: data.size as u64 })
}

/// Fréquence au format flottant 80 bits (IEEE 754 étendu) exigé par l'AIFF.
fn extended(rate: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate == 0 {
        return out;
    }
    let exp = 31 - rate.leading_zeros(); // position du bit de poids fort
    let mantissa = (rate as u64) << (63 - exp);
    let e = (16383 + exp) as u16;
    out[0..2].copy_from_slice(&e.to_be_bytes());
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// Écrit l'AIFF : en-têtes, échantillons (ordre des octets inversé), puis le tag ID3 recopié.
fn write_aiff(src: &Path, pcm: &Pcm, id3: Option<&[u8]>, dst: &Path) -> Result<(), String> {
    let bytes_per = (pcm.bits as u64).div_ceil(8);
    let frame = bytes_per * pcm.channels as u64;
    let data_len = pcm.data_len - pcm.data_len % frame; // octets d'une trame incomplète ignorés
    let frames = data_len / frame;
    if data_len + 46 > u32::MAX as u64 {
        return Err("fichier trop gros pour l'AIFF (4 Go)".into());
    }
    let id3_len = id3.map(|d| 8 + d.len() as u64 + (d.len() as u64 & 1)).unwrap_or(0);
    let form_len = 4 + (8 + 18) + (8 + 8 + data_len) + (data_len & 1) + id3_len;
    let mut out = BufWriter::new(File::create(dst).map_err(|e| e.to_string())?);
    let w = |out: &mut BufWriter<File>, b: &[u8]| out.write_all(b).map_err(|e| e.to_string());
    w(&mut out, b"FORM")?;
    w(&mut out, &(form_len as u32).to_be_bytes())?;
    w(&mut out, b"AIFF")?;
    w(&mut out, b"COMM")?;
    w(&mut out, &18u32.to_be_bytes())?;
    w(&mut out, &pcm.channels.to_be_bytes())?;
    w(&mut out, &(frames as u32).to_be_bytes())?;
    w(&mut out, &pcm.bits.to_be_bytes())?;
    w(&mut out, &extended(pcm.rate))?;
    w(&mut out, b"SSND")?;
    w(&mut out, &((8 + data_len) as u32).to_be_bytes())?;
    w(&mut out, &[0u8; 8])?; // offset et taille de bloc : 0
    let mut f = BufReader::new(File::open(src).map_err(|e| e.to_string())?);
    f.seek(SeekFrom::Start(pcm.data_offset)).map_err(|e| e.to_string())?;
    let step = (frame * 4096) as usize;
    let mut buf = vec![0u8; step];
    let mut left = data_len;
    while left > 0 {
        let n = (left as usize).min(step);
        f.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
        for s in buf[..n].chunks_exact_mut(bytes_per as usize) {
            if bytes_per == 1 {
                s[0] ^= 0x80; // WAV 8 bits non signé → AIFF 8 bits signé (même valeur)
            } else {
                s.reverse(); // petit-boutiste → gros-boutiste
            }
        }
        w(&mut out, &buf[..n])?;
        left -= n as u64;
    }
    if data_len & 1 == 1 {
        w(&mut out, &[0])?;
    }
    if let Some(d) = id3 {
        w(&mut out, b"ID3 ")?;
        w(&mut out, &(d.len() as u32).to_be_bytes())?;
        w(&mut out, d)?;
        if d.len() & 1 == 1 {
            w(&mut out, &[0])?;
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    out.get_ref().sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

/// Compare l'audio décodé des deux fichiers, échantillon par échantillon.
pub fn same_decoded_audio(a: &Path, b: &Path) -> Result<u64, String> {
    let mut da = crate::audio::decode::Decoder::open(a)?;
    let mut db = crate::audio::decode::Decoder::open(b)?;
    if da.channels != db.channels || da.sample_rate != db.sample_rate {
        return Err("format différent".into());
    }
    let (mut ba, mut bb) = (Vec::new(), Vec::new());
    let (mut qa, mut qb): (std::collections::VecDeque<f32>, std::collections::VecDeque<f32>) = Default::default();
    let mut compared = 0u64;
    let (mut ea, mut eb) = (false, false);
    loop {
        while qa.len() < 8192 && !ea {
            if da.next_block(&mut ba)? {
                qa.extend(ba.iter());
            } else {
                ea = true;
            }
        }
        while qb.len() < 8192 && !eb {
            if db.next_block(&mut bb)? {
                qb.extend(bb.iter());
            } else {
                eb = true;
            }
        }
        let n = qa.len().min(qb.len());
        for _ in 0..n {
            let (x, y) = (qa.pop_front().unwrap_or(0.0), qb.pop_front().unwrap_or(0.0));
            if x.to_bits() != y.to_bits() {
                return Err(format!("échantillon {compared} différent"));
            }
            compared += 1;
        }
        if ea && eb {
            break;
        }
        if n == 0 && (ea || eb) {
            break;
        }
    }
    if !qa.is_empty() || !qb.is_empty() || da.decode_errors > 0 || db.decode_errors > 0 {
        return Err("longueurs différentes".into());
    }
    Ok(compared)
}

fn target_rel(rel: &str) -> String {
    match rel.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.aiff"),
        None => format!("{rel}.aiff"),
    }
}

/// Aperçu : ce qui sera converti, et pourquoi certains fichiers ne le seront pas.
pub fn plan(lib: &Library, ids: &[i64]) -> Result<Vec<ConvertItem>, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let mounts = lib.mounts();
    let mut out = Vec::new();
    for &id in ids {
        let row: Option<(i64, String, String, String)> = conn
            .query_row("SELECT volume_id, rel_path, file_name, format FROM tracks WHERE id = ?1 AND status = 'ok'", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((vol, rel, name, format)) = row else { continue };
        let target = target_rel(&rel);
        let mut it = ConvertItem {
            track_id: id,
            file_name: name,
            target_name: target.rsplit('/').next().unwrap_or(&target).to_string(),
            status: "ok".into(),
            detail: None,
            in_rekordbox: crate::rekordbox::in_rekordbox(&conn, id),
        };
        if format != "wav" {
            it.status = "not_wav".into();
        } else if let Some(m) = mounts.get(&vol) {
            if volumes::join_rel(m, &target).exists() {
                it.status = "exists".into();
            } else if let Err(e) = wav_pcm(&volumes::join_rel(m, &rel)) {
                it.status = "unsupported".into();
                it.detail = Some(e);
            }
        } else {
            it.status = "offline".into();
        }
        out.push(it);
    }
    Ok(out)
}

fn fail(id: i64, name: &str, code: &str, msg: impl Into<String>) -> EditFailure {
    EditFailure { track_id: id, file_name: name.into(), code: code.into(), message: msg.into() }
}

/// Convertit les WAV demandés (l'aperçu est refait ici, jamais repris tel quel).
pub fn convert(lib: &std::sync::Arc<Library>, ids: &[i64], task: Option<&TaskHandle>) -> Result<EditReport, AppError> {
    let items = plan(lib, ids)?;
    let conn = crate::db::open_worker(lib.db_path())?;
    let batch = history::begin_batch(&conn, "history.wavToAiff", None)?;
    let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
    if let Some(t) = task {
        t.set_total(items.len() as u64);
    }
    let exceptions = index::artist_exceptions(&conn)?;
    let ws = crate::library::edit::write_settings(&conn);
    for (n, it) in items.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        if it.status != "ok" {
            report.failures.push(fail(it.track_id, &it.file_name, &format!("convert_{}", it.status), it.detail.clone().unwrap_or_default()));
            continue;
        }
        match convert_one(lib, &conn, it.track_id, batch, &exceptions, &ws) {
            Ok(()) => report.ok += 1,
            Err(f) => report.failures.push(f),
        }
        if let Some(t) = task {
            t.progress(n as u64 + 1);
        }
    }
    if history::drop_if_empty(&conn, batch)? {
        report.batch_id = None;
    }
    lib.notify_changed(true);
    lib.spawn_hash();
    Ok(report)
}

fn convert_one(
    lib: &Library,
    conn: &rusqlite::Connection,
    id: i64,
    batch: i64,
    exceptions: &[String],
    ws: &crate::tags::write::WriteSettings,
) -> Result<(), EditFailure> {
    let (vol, root_id, rel, name): (i64, Option<i64>, String, String) = conn
        .query_row("SELECT volume_id, root_id, rel_path, file_name FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|e| fail(id, "", "internal", e.to_string()))?;
    let mount = lib.mounts().get(&vol).cloned().ok_or_else(|| fail(id, &name, "offline", "disque déconnecté"))?;
    let src = volumes::join_rel(&mount, &rel);
    let aiff_rel = target_rel(&rel);
    let dst = volumes::join_rel(&mount, &aiff_rel);
    if dst.exists() {
        return Err(fail(id, &name, "exists", "un AIFF du même nom existe déjà"));
    }
    let tmp: PathBuf = dst.with_file_name(format!(".djlm-conv-{id}.aiff"));
    let cleanup = |e: String, code: &str| {
        let _ = std::fs::remove_file(&tmp);
        fail(id, &name, code, e)
    };
    let pcm = wav_pcm(&src).map_err(|e| fail(id, &name, "convert_unsupported", e))?;
    let id3 = {
        let mut f = BufReader::new(File::open(&src).map_err(|e| fail(id, &name, "io", e.to_string()))?);
        let layout = read_layout(&mut f).map_err(|e| fail(id, &name, "io", e.to_string()))?;
        crate::tags::chunks::read_id3_chunk(&layout, &mut f).map_err(|e| fail(id, &name, "io", e.to_string()))?
    };
    write_aiff(&src, &pcm, id3.as_deref(), &tmp).map_err(|e| cleanup(e, "io"))?;
    // Tags complets (RIFF INFO compris) et pochette : réécrits proprement dans l'AIFF.
    let wav_info = crate::tags::read::read_file(&src, exceptions).map_err(|e| cleanup(e.to_string(), "unreadable"))?;
    let aiff_now = crate::tags::read::read_file(&tmp, exceptions).map_err(|e| cleanup(e.to_string(), "verify"))?;
    let t = &wav_info.tags;
    let mut changes = Changes {
        title: Some(t.title.clone()),
        version: Some(t.version.clone()),
        artists: Some(t.artists.clone()),
        album: Some(t.album.clone()),
        year: Some(t.year),
        genre: Some(t.genre.clone()),
        bpm: Some(t.bpm),
        key: Some(t.key.clone()),
        rating: Some(t.rating),
        ..Default::default()
    };
    if aiff_now.cover.count == 0 && wav_info.cover.count > 0 {
        if let Ok(Some(c)) = crate::tags::write::current_cover(&src) {
            changes.cover = Some(Some(c));
        }
    }
    crate::tags::write::write_tags(&tmp, &aiff_now.tags, &changes, ws, exceptions).map_err(|e| cleanup(e.to_string(), e.code()))?;
    // Vérification : audio décodé identique, échantillon par échantillon.
    same_decoded_audio(&src, &tmp).map_err(|e| cleanup(format!("vérification de l'audio : {e}"), "verify"))?;
    std::fs::rename(&tmp, &dst).map_err(|e| cleanup(e.to_string(), "io"))?;
    log::info!("Converti en AIFF : {} → {}", src.display(), dst.display());

    let res = (|| -> Result<(), AppError> {
        let meta = std::fs::metadata(&dst)?;
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        let info = crate::tags::read::read_file(&dst, exceptions);
        let new_id = index::store(conn, vol, root_id, &aiff_rel, meta.len() as i64, mtime, info)?;
        // Le nouveau fichier reprend les cadenas et les playlists du WAV.
        conn.execute(
            "INSERT OR REPLACE INTO field_state (track_id, field, locked, source) SELECT ?2, field, locked, source FROM field_state WHERE track_id = ?1",
            params![id, new_id],
        )?;
        conn.execute("UPDATE OR IGNORE playlist_tracks SET track_id = ?2 WHERE track_id = ?1", params![id, new_id])?;
        // Le WAV d'origine part en quarantaine (restaurable)…
        let q = lib.quarantine_in(conn, &[id], "wav_to_aiff", batch)?;
        if q.ok == 0 {
            return Err(AppError::InvalidSetting(q.failures.first().map(|f| f.message.clone()).unwrap_or_default()));
        }
        // … et l'annulation mettra l'AIFF en quarantaine avant de rendre le WAV.
        let root_rel: Option<String> = conn.query_row("SELECT rel_path FROM roots WHERE id = ?1", [root_id], |r| r.get(0)).optional()?;
        let qrel = match root_rel.as_deref().filter(|r| !r.is_empty() && aiff_rel.starts_with(&format!("{r}/"))) {
            Some(r) => format!("{r}/{}/{}-aiff/{}", index::QUARANTINE_DIR, crate::db::today(), &aiff_rel[r.len() + 1..]),
            None => format!("{}/{}-aiff/{aiff_rel}", index::QUARANTINE_DIR, crate::db::today()),
        };
        let before = QuarantineState { volume_id: vol, rel_path: qrel, quarantined: true, reason: Some("wav_to_aiff".into()) };
        let after = QuarantineState { volume_id: vol, rel_path: aiff_rel.clone(), quarantined: false, reason: None };
        history::record(conn, batch, "quarantine", Some(new_id), &serde_json::to_string(&before).unwrap_or_default(), &serde_json::to_string(&after).unwrap_or_default())?;
        Ok(())
    })();
    res.map_err(|e| fail(id, &name, "internal", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::tests_support::library_on;
    use crate::testdata;

    /// WAV 24 bits stéréo.
    fn wav24(frames: usize) -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();
        for i in 0..frames {
            for ch in 0..2 {
                let v = (((i as f64 * 0.01 + ch as f64).sin()) * 8_000_000.0) as i32;
                data.extend(&v.to_le_bytes()[0..3]);
            }
        }
        let mut out = b"RIFF".to_vec();
        out.extend(((4 + 8 + 16 + 8 + data.len()) as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(2u16.to_le_bytes());
        out.extend(48_000u32.to_le_bytes());
        out.extend((48_000u32 * 6).to_le_bytes());
        out.extend(6u16.to_le_bytes());
        out.extend(24u16.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn frequence_80_bits() {
        assert_eq!(extended(44_100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(48_000)[0..4], [0x40, 0x0E, 0xBB, 0x80]);
    }

    #[test]
    fn conversion_identique_avec_tags_puis_annulation() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        let wav = testdata::write(&music, "a.wav", &testdata::wav_bytes(5));
        testdata::write(&music, "b24.wav", &wav24(20_000));
        let (lib, _) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let id_a: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'a.wav'", [], |r| r.get(0)).unwrap();
        let id_b: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'b24.wav'", [], |r| r.get(0)).unwrap();
        // Tags et pochette sur le WAV.
        let cover = crate::tags::write::CoverData { mime: "image/png".into(), data: crate::artwork::test_png(32, 32) };
        lib.edit(
            &[id_a],
            Changes { title: Some(Some("Été".into())), artists: Some(vec!["Zoë".into()]), rating: Some(4), cover: Some(Some(cover)), ..Default::default() },
            crate::library::edit::Source::Manual,
            "history.editTags",
            None,
        )
        .unwrap();
        let wav_bytes = std::fs::read(&wav).unwrap();

        let plan = plan(&lib, &[id_a, id_b]).unwrap();
        assert!(plan.iter().all(|p| p.status == "ok"), "{plan:?}");
        let r = convert(&lib, &[id_a, id_b], None).unwrap();
        assert_eq!(r.ok, 2, "{:?}", r.failures);
        let aiff = music.join("a.aiff");
        assert!(aiff.exists() && music.join("b24.aiff").exists());
        assert!(!wav.exists(), "le WAV est en quarantaine");
        // Audio décodé identique (le WAV est relu dans la quarantaine).
        let q = lib.quarantine_list().unwrap();
        let qa = q.iter().find(|i| i.file_name == "a.wav").unwrap();
        let qpath = PathBuf::from(qa.path.clone().unwrap());
        assert_eq!(std::fs::read(&qpath).unwrap(), wav_bytes, "le WAV d'origine est intact");
        assert!(same_decoded_audio(&qpath, &aiff).unwrap() > 0);
        let info = crate::tags::read::read_file(&aiff, &[]).unwrap();
        assert_eq!(info.tags.title.as_deref(), Some("Été"));
        assert_eq!(info.tags.artists, vec!["Zoë".to_string()]);
        assert_eq!(info.tags.rating, 4);
        assert_eq!(info.cover.count, 1, "la pochette suit");
        assert_eq!(info.bit_depth, Some(16));
        let b24 = crate::tags::read::read_file(&music.join("b24.aiff"), &[]).unwrap();
        assert_eq!((b24.bit_depth, b24.sample_rate), (Some(24), Some(48_000)));

        // Une seule annulation : l'AIFF part en quarantaine, le WAV revient, identique.
        let rep = lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(rep.failures.is_empty(), "{:?}", rep.failures);
        assert!(!aiff.exists());
        assert_eq!(std::fs::read(&wav).unwrap(), wav_bytes);
    }

    #[test]
    fn wav_flottant_refuse() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = testdata::wav_bytes(1);
        b[20] = 3; // codage « flottant »
        b[34] = 32;
        let p = testdata::write(dir.path(), "f.wav", &b);
        assert_eq!(wav_pcm(&p).err().as_deref(), Some("float"));
    }
}
