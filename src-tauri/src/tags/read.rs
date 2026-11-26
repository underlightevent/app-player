//! Lecture d'un fichier audio : propriétés techniques et tags, vers le modèle interne.

use std::fs::File;
use std::path::Path;

use lofty::config::{ParseOptions, ParsingMode};
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::picture::{PictureInformation, PictureType};
use lofty::tag::{Accessor, ItemKey, Tag, TagExt, TagType};

use super::chunks;
use super::text::{normalize_key, split_artists, split_title_version};
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
    let rating = tag.ratings().next().map(|p| p.rating as u8).unwrap_or(0).min(5);
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
pub fn read_file(path: &Path, exceptions: &[String]) -> Result<FileInfo, ReadError> {
    let mut file = File::open(path)?;
    let probe = lofty::probe::Probe::new(&mut file)
        .options(ParseOptions::new().parsing_mode(ParsingMode::BestAttempt))
        .guess_file_type()
        .map_err(|e| ReadError::Parse(e.to_string()))?;
    let tagged = probe.read().map_err(|e| ReadError::Parse(e.to_string()))?;
    let ft = tagged.file_type();
    let props = tagged.properties();

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
