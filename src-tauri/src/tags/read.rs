//! Lecture d'un fichier audio : propriétés techniques et tags, vers le modèle interne.

use std::fs::File;
use std::path::Path;

use lofty::config::{ParseOptions, ParsingMode};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::properties::FileProperties;
use lofty::picture::{PictureInformation, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};

use super::chunks;
use super::text::{normalize_key, rating_text_to_stars, split_artists, split_title_version, strip_artist_prefix};
use super::{CoverInfo, FileInfo, TrackTags};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("fichier introuvable ou inaccessible : {0}")]
    Io(#[from] std::io::Error),
    #[error("fichier illisible ou corrompu : {0}")]
    Parse(String),
}

fn format_name(ft: &FileType, path: &Path) -> String {
    match ft {
        FileType::Wav => "wav",
        FileType::Mpeg => "mp3",
        FileType::Flac => "flac",
        FileType::Aiff => "aiff",
        FileType::Mp4 => "m4a",
        FileType::Opus => "opus",
        FileType::Vorbis => "ogg",
        _ => return path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase(),
    }
    .to_string()
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(|v| v.trim().trim_matches('\0').trim().to_string()).filter(|v| !v.is_empty())
}

fn parse_year(s: &str) -> Option<i32> {
    let digits: String = s.trim().chars().take(4).collect();
    digits.parse::<i32>().ok().filter(|y| (1000..=2999).contains(y))
}

fn parse_bpm(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse::<f64>().ok().filter(|v| *v > 0.0 && *v < 1000.0)
}

/// Note en étoiles. ID3 : l'entrée POPM de l'application (échelle de l'Explorateur Windows)
/// est prioritaire sur celles des autres logiciels. Vorbis : champ RATING (0–5, 0–10 ou 0–100).
fn read_rating(tag: &Tag) -> u8 {
    if tag.tag_type() == TagType::VorbisComments {
        return tag.get_string(ItemKey::Popularimeter).and_then(rating_text_to_stars).unwrap_or(0);
    }
    let all: Vec<_> = tag.ratings().collect();
    all.iter()
        .find(|p| p.email() == super::write::POPM_EMAIL)
        .or_else(|| all.first())
        .map(|p| (p.rating as u8).min(5))
        .unwrap_or(0)
}

/// Extrait les champs du modèle depuis un tag générique lofty.
fn tags_from(tag: &Tag, exceptions: &[String]) -> TrackTags {
    let raw_title = clean(tag.get_string(ItemKey::TrackTitle));
    let subtitle = clean(tag.get_string(ItemKey::TrackSubtitle));
    let (title, version) = match &raw_title {
        Some(t) => {
            let (t, v) = split_title_version(t, subtitle.as_deref());
            (Some(t).filter(|t| !t.is_empty()), v)
        }
        None => (None, subtitle),
    };
    // Plusieurs valeurs d'artiste (FLAC, ID3v2.4) ou une seule à découper.
    let artist_values: Vec<&str> = tag.get_strings(ItemKey::TrackArtist).collect();
    let mut artists = Vec::new();
    for v in artist_values {
        for a in split_artists(v, exceptions) {
            if !artists.iter().any(|x: &String| x.eq_ignore_ascii_case(&a)) {
                artists.push(a);
            }
        }
    }
    let year = tag
        .get_string(ItemKey::Year)
        .and_then(parse_year)
        .or_else(|| tag.get_string(ItemKey::RecordingDate).and_then(parse_year))
        .or_else(|| tag.date().map(|d| d.year as i32))
        .or_else(|| tag.get_string(ItemKey::ReleaseDate).and_then(parse_year));
    let bpm = tag
        .get_string(ItemKey::Bpm)
        .and_then(parse_bpm)
        .or_else(|| tag.get_string(ItemKey::IntegerBpm).and_then(parse_bpm));
    let key = tag.get_string(ItemKey::InitialKey).and_then(normalize_key);
    let rating = read_rating(tag);
    TrackTags {
        title,
        version,
        artists,
        album: clean(tag.get_string(ItemKey::AlbumTitle)),
        year,
        genre: clean(tag.genre().as_deref()),
        bpm,
        key,
        rating,
        remixer: clean(tag.get_string(ItemKey::Remixer)),
        label: clean(tag.get_string(ItemKey::Label)),
    }
}

fn cover_from(tag: Option<&Tag>) -> CoverInfo {
    let Some(tag) = tag else { return CoverInfo::default() };
    let pictures = tag.pictures();
    let chosen = pictures
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first());
    let info = chosen.and_then(|p| PictureInformation::from_picture(p).ok());
    CoverInfo {
        count: pictures.len() as u32,
        width: info.as_ref().map(|i| i.width),
        height: info.as_ref().map(|i| i.height),
    }
}

/// Ordre de préférence des tags selon le format.
fn preferred_tag_types(ft: &FileType) -> &'static [TagType] {
    match ft {
        FileType::Wav => &[TagType::Id3v2, TagType::RiffInfo],
        FileType::Mpeg => &[TagType::Id3v2, TagType::Ape, TagType::Id3v1],
        FileType::Aiff => &[TagType::Id3v2, TagType::AiffText],
        FileType::Flac => &[TagType::VorbisComments, TagType::Id3v2],
        FileType::Mp4 => &[TagType::Mp4Ilst],
        _ => &[TagType::VorbisComments, TagType::Id3v2, TagType::Ape],
    }
}

/// Lit un fichier. `exceptions` : noms d'artistes à ne jamais découper.
fn parse_opts() -> ParseOptions {
    ParseOptions::new().parsing_mode(ParsingMode::BestAttempt)
}

/// Ouvre un fichier avec lofty, avec deux solutions de secours pour les fichiers que
/// les lecteurs courants lisent sans problème :
/// 1. type de fichier impossible à deviner (remplissage après l'ID3, par exemple) :
///    on se fie à l'extension ;
/// 2. MP3 dont un tag de fin (APE) est abîmé : les tags du début (ID3v2) et de la fin
///    (ID3v1) sont lus à part, les propriétés sur le fichier entier sans les tags.
/// Rien n'est jamais écrit ici.
fn open_tagged(path: &Path) -> Result<(TaggedFile, FileProperties), ReadError> {
    let first = (|| -> Result<TaggedFile, String> {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let probe = Probe::new(&mut f).options(parse_opts()).guess_file_type().map_err(|e| e.to_string())?;
        probe.read().map_err(|e| e.to_string())
    })();
    let first_err = match first {
        Ok(t) => {
            let p = t.properties().clone();
            return Ok((t, p));
        }
        Err(e) => e,
    };
    File::open(path)?; // fichier absent ou inaccessible : erreur claire
    let Some(ft) = FileType::from_path(path) else { return Err(ReadError::Parse(first_err)) };
    let mut f = File::open(path)?;
    if let Ok(t) = Probe::new(&mut f).options(parse_opts()).set_file_type(ft).read() {
        let p = t.properties().clone();
        return Ok((t, p));
    }
    if ft == FileType::Mpeg {
        if let Some(r) = salvage_mpeg(path) {
            return Ok(r);
        }
    }
    Err(ReadError::Parse(first_err))
}

/// MP3 au tag de fin abîmé : tags lus sur une copie en mémoire réduite au tag ID3v2,
/// au début du son et au tag ID3v1 ; propriétés lues sur le fichier entier sans les tags.
fn salvage_mpeg(path: &Path) -> Option<(TaggedFile, FileProperties)> {
    let mut f = File::open(path).ok()?;
    let props = lofty::mpeg::MpegFile::read_from(&mut f, parse_opts().read_tags(false)).ok()?.properties().clone();
    let tagged = Probe::new(std::io::Cursor::new(salvage_bytes(path, true)?)).options(parse_opts()).set_file_type(FileType::Mpeg).read().ok()?;
    Some((tagged, props.into()))
}

/// Tag ID3v2 complet + 256 Kio de son (+ tag ID3v1 s'il existe et si `with_v1`), sans le reste de la fin du fichier.
pub(crate) fn salvage_bytes(path: &Path, with_v1: bool) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut head = [0u8; 10];
    f.read_exact(&mut head).ok()?;
    let tag_end = if &head[..3] == b"ID3" {
        let size = head[6..10].iter().fold(0u64, |a, b| (a << 7) | (*b as u64 & 0x7f));
        10 + size + if head[5] & 0x10 != 0 { 10 } else { 0 }
    } else {
        0
    };
    // Fin du son : avant l'ID3v1 (128 octets « TAG ») et avant un tag APE éventuel.
    let mut v1 = [0u8; 128];
    let has_v1 = len >= tag_end + 128
        && f.seek(SeekFrom::Start(len - 128)).is_ok()
        && f.read_exact(&mut v1).is_ok()
        && &v1[..3] == b"TAG";
    let mut audio_end = if has_v1 { len - 128 } else { len };
    if audio_end >= tag_end + 32 {
        let mut footer = [0u8; 32];
        f.seek(SeekFrom::Start(audio_end - 32)).ok()?;
        f.read_exact(&mut footer).ok()?;
        if &footer[..8] == b"APETAGEX" {
            let size = u32::from_le_bytes(footer[12..16].try_into().ok()?) as u64;
            let header = if footer[23] & 0x80 != 0 { 32 } else { 0 };
            audio_end = audio_end.saturating_sub(size + header).max(tag_end);
        }
    }
    let keep = (tag_end + 256 * 1024).min(audio_end);
    let mut buf = vec![0u8; keep as usize];
    f.seek(SeekFrom::Start(0)).ok()?;
    f.read_exact(&mut buf).ok()?;
    if has_v1 && with_v1 {
        buf.extend_from_slice(&v1);
    }
    Some(buf)
}

pub fn read_file(path: &Path, exceptions: &[String]) -> Result<FileInfo, ReadError> {
    let (tagged, props) = open_tagged(path)?;
    let props = &props;
    let ft = tagged.file_type();

    let main_tag = preferred_tag_types(&ft).iter().find_map(|t| tagged.tag(*t).filter(|t| !t.is_empty()));
    let mut tags = main_tag.map(|t| tags_from(t, exceptions)).unwrap_or_default();

    // Champs manquants complétés par les autres tags du fichier.
    for t in preferred_tag_types(&ft).iter().filter_map(|t| tagged.tag(*t)) {
        let other = tags_from(t, exceptions);
        if tags.title.is_none() {
            tags.title = other.title;
            if tags.version.is_none() {
                tags.version = other.version;
            }
        }
        if tags.artists.is_empty() {
            tags.artists = other.artists;
        }
        tags.album = tags.album.or(other.album);
        tags.year = tags.year.or(other.year);
        tags.genre = tags.genre.or(other.genre);
        tags.bpm = tags.bpm.or(other.bpm);
        tags.key = tags.key.or(other.key);
        if tags.rating == 0 {
            tags.rating = other.rating;
        }
    }

    let cover_tag = preferred_tag_types(&ft).iter().find_map(|t| tagged.tag(*t).filter(|t| !t.pictures().is_empty()));
    let cover = cover_from(cover_tag);

    let mut wav_desync = false;
    if ft == FileType::Wav {
        wav_desync = wav_info_differs(path, tagged.tag(TagType::Id3v2), exceptions).unwrap_or(false);
        if tagged.tag(TagType::Id3v2).is_none() {
            // Pas d'ID3 : on relit RIFF INFO nous-mêmes (lofty ignore les textes non UTF-8).
            if let Ok(info) = read_riff_info(path) {
                let get = |id: &[u8; 4]| info.iter().find(|i| &i.id == id).map(|i| chunks::decode_text(&i.raw));
                if tags.title.is_none() {
                    if let Some(t) = get(b"INAM") {
                        let (t, v) = split_title_version(&t, None);
                        tags.title = Some(t).filter(|t| !t.is_empty());
                        tags.version = v;
                    }
                }
                if tags.artists.is_empty() {
                    tags.artists = get(b"IART").map(|a| split_artists(&a, exceptions)).unwrap_or_default();
                }
                tags.album = tags.album.or_else(|| clean(get(b"IPRD").as_deref()));
                tags.genre = tags.genre.or_else(|| clean(get(b"IGNR").as_deref()));
                tags.year = tags.year.or_else(|| get(b"ICRD").and_then(|y| parse_year(&y)));
            }
        }
    }

    // Titre écrit « Artiste - Titre » (réglage « artiste dans le titre ») : on retrouve le titre seul.
    if let (Some(t), Some(a)) = (tags.title.as_deref(), tags.artists.first()) {
        tags.title = Some(strip_artist_prefix(t, a));
    }
    let raw_title = main_tag
        .and_then(|t| t.get_string(ItemKey::TrackTitle))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let codec = match ft {
        FileType::Mp4 => Some(if props.bit_depth().is_some() { "ALAC" } else { "AAC" }.to_string()),
        FileType::Wav => Some("PCM".to_string()),
        FileType::Aiff => Some("PCM".to_string()),
        FileType::Flac => Some("FLAC".to_string()),
        FileType::Mpeg => Some("MP3".to_string()),
        FileType::Opus => Some("Opus".to_string()),
        FileType::Vorbis => Some("Vorbis".to_string()),
        _ => None,
    };

    Ok(FileInfo {
        format: format_name(&ft, path),
        codec,
        bitrate: props.audio_bitrate().or(props.overall_bitrate()).filter(|b| *b > 0),
        sample_rate: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        duration_ms: props.duration().as_millis() as u64,
        tags,
        cover,
        wav_desync,
        raw_title,
    })
}

pub fn read_riff_info(path: &Path) -> std::io::Result<Vec<chunks::InfoItem>> {
    let mut f = File::open(path)?;
    let layout = chunks::read_layout(&mut f)?;
    Ok(chunks::find_info(&layout, &mut f)?.map(|(_, items)| items).unwrap_or_default())
}

/// Compare les 4 champs communs entre RIFF INFO et ID3 (après normalisation).
fn wav_info_differs(path: &Path, id3: Option<&Tag>, exceptions: &[String]) -> std::io::Result<bool> {
    let info = read_riff_info(path)?;
    let Some(id3) = id3 else { return Ok(false) };
    if info.is_empty() {
        return Ok(!id3.is_empty());
    }
    let get = |id: &[u8; 4]| {
        info.iter().find(|i| &i.id == id).map(|i| chunks::decode_text(&i.raw)).filter(|s| !s.trim().is_empty())
    };
    let norm = |s: Option<String>| s.map(|v| super::text::comparable(&v)).unwrap_or_default();
    let id3_title = id3.get_string(ItemKey::TrackTitle).map(str::to_string);
    let id3_artist = {
        let all: Vec<String> = id3
            .get_strings(ItemKey::TrackArtist)
            .flat_map(|a| split_artists(a, exceptions))
            .collect();
        if all.is_empty() {
            None
        } else {
            Some(all.join(" "))
        }
    };
    let info_artist = get(b"IART").map(|a| split_artists(&a, exceptions).join(" "));
    Ok(norm(get(b"INAM")) != norm(id3_title)
        || norm(info_artist) != norm(id3_artist)
        || norm(get(b"IPRD")) != norm(id3.get_string(ItemKey::AlbumTitle).map(str::to_string))
        || norm(get(b"IGNR")) != norm(id3.genre().map(|g| g.to_string())))
}

/// Diagnostic en lecture seule d'un dossier réel (jamais lancé automatiquement) :
/// `DJLM_DIAG_DIR=E:\... cargo test diag_dossier -- --ignored --nocapture`
#[cfg(test)]
mod diag {
    #[test]
    #[ignore]
    fn diag_dossier() {
        let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
        let mut stack = vec![std::path::PathBuf::from(dir)];
        let (mut total, mut bad) = (0, 0);
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if !crate::tags::is_audio_path(&p) {
                    continue;
                }
                total += 1;
                if let Err(err) = super::read_file(&p, &[]) {
                    bad += 1;
                    let play = match crate::audio::decode::Decoder::open(&p) {
                        Ok(_) => "lecture OK".to_string(),
                        Err(e) => format!("lecture KO: {e}"),
                    };
                    let mut head = [0u8; 12];
                    let _ = std::io::Read::read(&mut std::fs::File::open(&p).unwrap(), &mut head);
                    println!("DIAG|{}|{}|{}|{:?}", p.display(), err, play, String::from_utf8_lossy(&head));
                }
            }
        }
        println!("DIAG total={total} illisibles={bad}");
    }
}

