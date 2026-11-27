//! Écriture sûre des tags (principe 2 du cahier des charges).
//!
//! 1. Le fichier est recopié dans un fichier temporaire du même dossier.
//! 2. Seuls les champs demandés sont modifiés dans la copie ; tout le reste
//!    (données Serato, Rekordbox, Beatport, commentaires…) est conservé.
//! 3. Relecture de contrôle : tags relus identiques aux tags voulus, et hash du
//!    flux audio identique à celui de l'original.
//! 4. Remplacement atomique de l'original par la copie.
//! En cas d'échec à n'importe quelle étape, l'original reste intact.

use std::borrow::Cow;
use std::fs::{self, File, OpenOptions};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use lofty::config::{ParseOptions, ParsingMode, WriteOptions};
use lofty::file::AudioFile;
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, PopularimeterFrame, TextInformationFrame, TimestampFrame};
use lofty::tag::items::Timestamp;
use lofty::iff::wav::WavFile;
use lofty::mp4::Mp4File;
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OggPictureStorage, OpusFile, VorbisFile};
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::tag::{ItemKey, MergeTag, SplitTag, TagExt};
use lofty::TextEncoding;
use serde::{Deserialize, Serialize};

use super::chunks::{self, InfoItem, Replacement};
use super::read::read_file;
use super::text::{full_title, join_artists, normalize_key, remixer_from_version, stars_to_popm, to_camelot};
use super::{FileInfo, TrackTags};
use crate::audio::hash::audio_stream_hash;

/// Adresse POPM utilisée par l'Explorateur Windows (et l'échelle 1/64/128/196/255).
pub const POPM_EMAIL: &str = "Windows Media Player 9 Series";

/// Réglages d'écriture (cahier des charges, « Règles d'écriture »).
#[derive(Debug, Clone)]
pub struct WriteSettings {
    pub version_in_title: bool,
    /// « Artiste principal - Titre » (les platines n'affichent souvent que le titre).
    pub artist_in_title: bool,
    /// « , », « & » ou « ; »
    pub artist_separator: String,
    pub key_camelot: bool,
    pub bpm_integer: bool,
    pub single_cover: bool,
}

impl Default for WriteSettings {
    fn default() -> Self {
        WriteSettings {
            version_in_title: true,
            artist_in_title: false,
            artist_separator: ",".into(),
            key_camelot: true,
            bpm_integer: true,
            single_cover: true,
        }
    }
}

/// Nouvelle pochette (JPEG prêt à écrire), ou suppression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverData {
    pub mime: String,
    #[serde(with = "serde_bytes_b64")]
    pub data: Vec<u8>,
}

mod serde_bytes_b64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::util::b64_encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        crate::util::b64_decode(&s).map_err(serde::de::Error::custom)
    }
}

/// Distingue « champ absent » (inchangé) de « champ à null » (vidé) en JSON.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer, T: Serialize>(v: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(Some(x)) => s.serialize_some(x),
            _ => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
        Option::<T>::deserialize(d).map(Some)
    }
}

/// Champs à modifier. `None` : champ inchangé. `Some(None)` : champ vidé.
/// En JSON : champ absent = inchangé ; `null` = vidé.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Changes {
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub title: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub version: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub album: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub year: Option<Option<i32>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub genre: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub bpm: Option<Option<f64>>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub key: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none", with = "double_option")]
    pub cover: Option<Option<CoverData>>,
    /// WAV : réécrire RIFF INFO depuis les valeurs complètes (correction d'une désynchronisation).
    #[serde(skip)]
    pub sync_wav: bool,
    /// Texte exact du champ titre du fichier (changement de présentation du titre et
    /// son annulation) : la valeur du titre en base ne change pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_title: Option<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.version.is_none()
            && self.artists.is_none()
            && self.album.is_none()
            && self.year.is_none()
            && self.genre.is_none()
            && self.bpm.is_none()
            && self.key.is_none()
            && self.rating.is_none()
            && self.cover.is_none()
            && !self.sync_wav
            && self.raw_title.is_none()
    }

    /// Noms des champs modifiés (pour l'historique et les cadenas).
    pub fn fields(&self) -> Vec<&'static str> {
        let mut f = Vec::new();
        if self.title.is_some() || self.raw_title.is_some() {
            f.push("title");
        }
        if self.version.is_some() {
            f.push("version");
        }
        if self.artists.is_some() {
            f.push("artists");
        }
        if self.album.is_some() {
            f.push("album");
        }
        if self.year.is_some() {
            f.push("year");
        }
        if self.genre.is_some() {
            f.push("genre");
        }
        if self.bpm.is_some() {
            f.push("bpm");
        }
        if self.key.is_some() {
            f.push("key");
        }
        if self.rating.is_some() {
            f.push("rating");
        }
        if self.cover.is_some() {
            f.push("cover");
        }
        f
    }

    /// Nettoie les valeurs : espaces, chaînes vides → champ vidé, tonalité normalisée.
    pub fn normalized(mut self) -> Self {
        fn txt(v: Option<Option<String>>) -> Option<Option<String>> {
            v.map(|o| o.map(|s| super::text::nfc(s.trim())).filter(|s| !s.is_empty()))
        }
        self.title = txt(self.title);
        self.version = txt(self.version);
        self.album = txt(self.album);
        self.genre = txt(self.genre);
        self.artists = self.artists.map(|a| {
            let mut out: Vec<String> = Vec::new();
            for x in a {
                let x = super::text::nfc(x.trim());
                if !x.is_empty() && !out.iter().any(|y| y.eq_ignore_ascii_case(&x)) {
                    out.push(x);
                }
            }
            out
        });
        self.key = self.key.map(|k| k.and_then(|k| normalize_key(&k)));
        self.year = self.year.map(|y| y.filter(|y| (1000..=2999).contains(y)));
        self.bpm = self.bpm.map(|b| b.filter(|b| *b > 0.0 && *b < 1000.0));
        self.rating = self.rating.map(|r| r.min(5));
        self
    }
}

/// Valeurs complètes après modification (état actuel + changements).
pub fn merged(current: &TrackTags, c: &Changes) -> TrackTags {
    let mut t = current.clone();
    if let Some(v) = &c.title {
        t.title = v.clone();
    }
    if let Some(v) = &c.version {
        t.version = v.clone();
    }
    if let Some(v) = &c.artists {
        t.artists = v.clone();
    }
    if let Some(v) = &c.album {
        t.album = v.clone();
    }
    if let Some(v) = c.year {
        t.year = v;
    }
    if let Some(v) = &c.genre {
        t.genre = v.clone();
    }
    if let Some(v) = c.bpm {
        t.bpm = v;
    }
    if let Some(v) = &c.key {
        t.key = v.clone();
    }
    if let Some(v) = c.rating {
        t.rating = v;
    }
    if c.version.is_some() {
        t.remixer = t.version.as_deref().and_then(remixer_from_version);
    }
    t
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("le fichier est en lecture seule")]
    ReadOnly,
    #[error("le fichier est utilisé par un autre programme")]
    InUse,
    #[error("plus assez de place sur le disque")]
    DiskFull,
    #[error("format non pris en charge pour l'écriture")]
    Unsupported,
    #[error("ce fichier contient des tags que l'application ne sait pas relire sans risque ; il n'a pas été modifié ({0})")]
    UnsafeTags(String),
    #[error("la relecture de contrôle a échoué ({0}) ; le fichier d'origine n'a pas été modifié")]
    Verify(String),
    #[error("le son du fichier aurait changé ; le fichier d'origine n'a pas été modifié")]
    AudioChanged,
    #[error("erreur de fichier : {0}")]
    Io(std::io::Error),
}

impl WriteError {
    pub fn code(&self) -> &'static str {
        match self {
            WriteError::ReadOnly => "read_only",
            WriteError::InUse => "in_use",
            WriteError::DiskFull => "disk_full",
            WriteError::Unsupported => "unsupported",
            WriteError::UnsafeTags(_) => "unsafe_tags",
            WriteError::Verify(_) => "verify",
            WriteError::AudioChanged => "audio_changed",
            WriteError::Io(_) => "io",
        }
    }
}

impl From<std::io::Error> for WriteError {
    fn from(e: std::io::Error) -> Self {
        classify_io(e)
    }
}

/// Traduit les erreurs système courantes en causes compréhensibles.
pub fn classify_io(e: std::io::Error) -> WriteError {
    use std::io::ErrorKind::*;
    #[cfg(windows)]
    {
        // 32 : partage refusé (fichier ouvert ailleurs) ; 33 : verrouillé ; 112 : disque plein ; 19 : protégé en écriture
        match e.raw_os_error() {
            Some(32) | Some(33) => return WriteError::InUse,
            Some(112) | Some(39) => return WriteError::DiskFull,
            Some(19) => return WriteError::ReadOnly,
            _ => {}
        }
    }
    #[cfg(unix)]
    {
        match e.raw_os_error() {
            Some(28) => return WriteError::DiskFull, // ENOSPC
            Some(30) => return WriteError::ReadOnly, // EROFS
            _ => {}
        }
    }
    match e.kind() {
        PermissionDenied => WriteError::ReadOnly,
        _ => WriteError::Io(e),
    }
}

fn lofty_err(e: lofty::error::FileParseError) -> WriteError {
    WriteError::UnsafeTags(e.to_string())
}

fn lofty_write_err(e: impl std::fmt::Display) -> WriteError {
    WriteError::Io(std::io::Error::other(e.to_string()))
}

/// Nom du fichier temporaire : caché (commence par un point), ignoré par l'indexation.
fn temp_path(path: &Path) -> PathBuf {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    path.with_file_name(format!(".djlm-tmp-{}-{nanos}.{ext}", std::process::id()))
}

struct TempGuard(PathBuf);
impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn parse_opts() -> ParseOptions {
    // Mode strict : si un tag ne se relit pas parfaitement, on refuse d'écrire.
    ParseOptions::new().parsing_mode(ParsingMode::Strict).read_properties(false)
}

/// Pochette de face actuelle (pour l'historique).
pub fn current_cover(path: &Path) -> Result<Option<CoverData>, WriteError> {
    use lofty::file::TaggedFileExt;
    let tagged = lofty::read_from_path(path).map_err(lofty_err)?;
    for tag in tagged.tags() {
        let pics = tag.pictures();
        if let Some(p) = pics.iter().find(|p| p.pic_type() == PictureType::CoverFront).or_else(|| pics.first()) {
            let mime = p.mime_type().map(|m| m.as_str().to_string()).unwrap_or_else(|| "image/jpeg".into());
            return Ok(Some(CoverData { mime, data: p.data().to_vec() }));
        }
    }
    Ok(None)
}

fn make_picture(c: &CoverData) -> Picture {
    Picture::unchecked(c.data.clone())
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::from_str(&c.mime))
        .build()
}

fn key_text(key: &str, s: &WriteSettings) -> String {
    if s.key_camelot {
        to_camelot(key).unwrap_or_else(|| key.to_string())
    } else {
        key.to_string()
    }
}

fn bpm_text(bpm: f64, s: &WriteSettings) -> String {
    if s.bpm_integer {
        format!("{}", bpm.round() as i64)
    } else {
        let t = format!("{bpm:.2}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ---------- ID3v2 (MP3, WAV, AIFF) ----------

fn fid(id: &'static str) -> FrameId<'static> {
    FrameId::Valid(Cow::Borrowed(id))
}

/// Valeur à écrire dans le champ titre, ou `None` s'il ne faut pas y toucher.
/// `force` : réécriture complète (resynchronisation des WAV).
fn title_text(t: &TrackTags, c: &Changes, s: &WriteSettings, force: bool) -> Option<Option<String>> {
    if let Some(raw) = &c.raw_title {
        return Some(Some(raw.trim().to_string()).filter(|r| !r.is_empty()));
    }
    let artist_changed = s.artist_in_title && c.artists.is_some();
    if force || c.title.is_some() || c.version.is_some() || artist_changed {
        return Some(t.title.as_deref().map(|ti| {
            full_title(ti, t.version.as_deref(), t.artists.first().map(String::as_str), s.version_in_title, s.artist_in_title)
        }));
    }
    None
}

fn set_text(tag: &mut Id3v2Tag, id: &'static str, value: Option<&str>) {
    let _ = tag.remove(&fid(id)).count();
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        tag.insert(Frame::Text(TextInformationFrame::new(fid(id), TextEncoding::UTF16, v.to_string())));
    }
}

fn apply_id3(tag: &mut Id3v2Tag, t: &TrackTags, c: &Changes, s: &WriteSettings) {
    if let Some(title) = title_text(t, c, s, false) {
        set_text(tag, "TIT2", title.as_deref());
    }
    if c.title.is_some() || c.version.is_some() {
        set_text(tag, "TIT3", t.version.as_deref());
    }
    if c.version.is_some() {
        // Remixeur déduit de la version ; retiré si la version n'en désigne plus.
        set_text(tag, "TPE4", t.remixer.as_deref());
    }
    if c.artists.is_some() {
        let joined = join_artists(&t.artists, &s.artist_separator);
        set_text(tag, "TPE1", Some(&joined));
    }
    if c.album.is_some() {
        set_text(tag, "TALB", t.album.as_deref());
    }
    if c.genre.is_some() {
        set_text(tag, "TCON", t.genre.as_deref());
    }
    if c.year.is_some() {
        for id in ["TYER", "TDRC", "TDAT", "TORY"] {
            let _ = tag.remove(&fid(id)).count();
        }
        if let Some(y) = t.year {
            // Cadre « horodatage » TDRC : lofty le convertit en TYER à l'écriture ID3v2.3.
            let ts = Timestamp { year: y as u16, ..Timestamp::default() };
            tag.insert(Frame::Timestamp(TimestampFrame::new(fid("TDRC"), TextEncoding::UTF16, ts)));
        }
    }
    if c.bpm.is_some() {
        set_text(tag, "TBPM", t.bpm.map(|b| bpm_text(b, s)).as_deref());
    }
    if c.key.is_some() {
        set_text(tag, "TKEY", t.key.as_deref().map(|k| key_text(k, s)).as_deref());
    }
    if let Some(r) = c.rating {
        // On ne touche qu'à notre entrée POPM ; celles des autres logiciels restent.
        let others: Vec<Frame<'static>> = tag
            .iter()
            .filter(|f| matches!(f, Frame::Popularimeter(p) if p.email != POPM_EMAIL))
            .cloned()
            .collect();
        tag.retain(|f| !matches!(f, Frame::Popularimeter(_)));
        for f in others {
            tag.insert(f);
        }
        if r > 0 {
            tag.insert(Frame::Popularimeter(PopularimeterFrame::new(POPM_EMAIL, stars_to_popm(r), 0)));
        }
    }
    if let Some(cover) = &c.cover {
        if s.single_cover {
            tag.retain(|f| !matches!(f, Frame::Picture(_)));
        } else {
            tag.remove_picture_type(PictureType::CoverFront);
        }
        if let Some(cd) = cover {
            tag.insert_picture(make_picture(cd));
        }
    }
}

/// Lit un bloc ID3 brut (contenu d'un bloc « id3 » de WAV/AIFF) en mode strict,
/// en le plaçant dans un WAV minimal en mémoire.
fn parse_id3_block(raw: &[u8]) -> Result<Id3v2Tag, WriteError> {
    let fmt: [u8; 16] = [1, 0, 2, 0, 0x44, 0xac, 0, 0, 0x10, 0xb1, 2, 0, 4, 0, 16, 0];
    let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    w.extend(16u32.to_le_bytes());
    w.extend(fmt);
    w.extend(b"data");
    w.extend(4u32.to_le_bytes());
    w.extend([0u8; 4]);
    w.extend(b"id3 ");
    w.extend((raw.len() as u32).to_le_bytes());
    w.extend(raw);
    if raw.len() % 2 == 1 {
        w.push(0);
    }
    let total = (w.len() - 8) as u32;
    w[4..8].copy_from_slice(&total.to_le_bytes());
    let file = WavFile::read_from(&mut Cursor::new(w), parse_opts()).map_err(lofty_err)?;
    Ok(file.id3v2().cloned().unwrap_or_default())
}

fn id3_bytes(tag: &Id3v2Tag, v23: bool) -> Result<Vec<u8>, WriteError> {
    let mut out = Vec::new();
    let mut opts = WriteOptions::default();
    let opts = opts.use_id3v23(v23);
    tag.dump_to(&mut out, opts).map_err(lofty_write_err)?;
    Ok(out)
}

// ---------- Vorbis (FLAC, OGG, Opus) ----------

fn vset(vc: &mut VorbisComments, key: &str, value: Option<String>) {
    let _ = vc.remove(key).count();
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        vc.insert(key.to_string(), v);
    }
}

fn apply_vorbis(vc: &mut VorbisComments, t: &TrackTags, c: &Changes, s: &WriteSettings) {
    if let Some(title) = title_text(t, c, s, false) {
        vset(vc, "TITLE", title);
    }
    if c.title.is_some() || c.version.is_some() {
        vset(vc, "SUBTITLE", t.version.clone());
    }
    if c.version.is_some() {
        vset(vc, "REMIXER", t.remixer.clone());
    }
    if c.artists.is_some() {
        vset(vc, "ARTIST", Some(join_artists(&t.artists, &s.artist_separator)));
    }
    if c.album.is_some() {
        vset(vc, "ALBUM", t.album.clone());
    }
    if c.genre.is_some() {
        vset(vc, "GENRE", t.genre.clone());
    }
    if c.year.is_some() {
        let _ = vc.remove("YEAR").count();
        vset(vc, "DATE", t.year.map(|y| y.to_string()));
    }
    if c.bpm.is_some() {
        vset(vc, "BPM", t.bpm.map(|b| bpm_text(b, s)));
    }
    if c.key.is_some() {
        let _ = vc.remove("KEY").count();
        vset(vc, "INITIALKEY", t.key.as_deref().map(|k| key_text(k, s)));
    }
    if let Some(r) = c.rating {
        // Échelle 0–100 (MusicBee, foobar2000, Mp3tag).
        vset(vc, "RATING", (r > 0).then(|| (r as u32 * 20).to_string()));
    }
}

fn apply_ogg_pictures<T: OggPictureStorage>(store: &mut T, c: &Changes, s: &WriteSettings) -> Result<(), WriteError> {
    if let Some(cover) = &c.cover {
        if s.single_cover {
            let _ = store.remove_pictures();
        } else {
            store.remove_picture_type(PictureType::CoverFront);
        }
        if let Some(cd) = cover {
            let pic = make_picture(cd);
            let info = PictureInformation::from_picture(&pic).unwrap_or_default();
            store.insert_picture(pic, Some(info)).map_err(lofty_write_err)?;
        }
    }
    Ok(())
}

// ---------- RIFF INFO (WAV) ----------

fn info_set(items: &mut Vec<InfoItem>, id: &[u8; 4], value: Option<&str>) {
    let v = value.unwrap_or("").as_bytes().to_vec();
    if let Some(it) = items.iter_mut().find(|i| &i.id == id) {
        it.raw = v; // vide : supprimé à l'écriture
    } else if !v.is_empty() {
        items.push(InfoItem { id: *id, raw: v });
    }
}

fn apply_info(items: &mut Vec<InfoItem>, t: &TrackTags, c: &Changes, s: &WriteSettings) {
    let all = c.sync_wav;
    if let Some(title) = title_text(t, c, s, all) {
        info_set(items, b"INAM", title.as_deref());
    }
    if all || c.artists.is_some() {
        info_set(items, b"IART", Some(&join_artists(&t.artists, &s.artist_separator)));
    }
    if all || c.album.is_some() {
        info_set(items, b"IPRD", t.album.as_deref());
    }
    if all || c.genre.is_some() {
        info_set(items, b"IGNR", t.genre.as_deref());
    }
    if all || c.year.is_some() {
        info_set(items, b"ICRD", t.year.map(|y| y.to_string()).as_deref());
    }
}

/// lofty relit le type de fichier depuis le début avant d'écrire.
fn rewind(f: &mut File) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    f.seek(SeekFrom::Start(0)).map(|_| ())
}

/// Écrit les changements dans `tmp`. Toute panique d'une bibliothèque est transformée en
/// erreur : l'application ne plante pas et l'original n'est pas touché.
fn write_into_temp(path: &Path, tmp: &Path, t: &TrackTags, c: &Changes, s: &WriteSettings) -> Result<(), WriteError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_into_temp_inner(path, tmp, t, c, s)))
        .unwrap_or_else(|_| Err(WriteError::UnsafeTags("erreur interne de la bibliothèque de tags".into())))
}

fn write_into_temp_inner(path: &Path, tmp: &Path, t: &TrackTags, c: &Changes, s: &WriteSettings) -> Result<(), WriteError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "wav" | "aif" | "aiff" => {
            let is_wav = ext == "wav";
            let src_bytes = fs::read(path)?;
            let mut cur = Cursor::new(&src_bytes);
            let layout = chunks::read_layout(&mut cur)?;
            // Tag ID3 actuel (lu en mode strict à part : RIFF INFO est traité par nos soins), sinon un tag neuf.
            let mut id3 = match chunks::read_id3_chunk(&layout, &mut cur)? {
                Some(raw) => parse_id3_block(&raw)?,
                None => Id3v2Tag::default(),
            };
            let v23 = is_wav; // WAV : ID3v2.3 ; AIFF : ID3v2.4
            apply_id3(&mut id3, t, c, s);
            // Tag devenu vide : le bloc ID3 est retiré plutôt qu'écrit vide.
            let id3_data = if id3.is_empty() { Vec::new() } else { id3_bytes(&id3, v23)? };
            let info_data = if is_wav {
                let mut items = chunks::find_info(&layout, &mut cur)?.map(|(_, i)| i).unwrap_or_default();
                apply_info(&mut items, t, c, s);
                Some(chunks::build_info(&items))
            } else {
                None
            };
            let mut out = std::io::BufWriter::new(File::create(tmp)?);
            chunks::rewrite(
                &mut cur,
                &layout,
                &Replacement { info: info_data.as_deref(), id3: Some(&id3_data) },
                &mut out,
            )?;
            let f = out.into_inner().map_err(|e| WriteError::Io(e.into_error()))?;
            f.sync_all()?;
        }
        "mp3" => {
            fs::copy(path, tmp)?;
            let mut f = OpenOptions::new().read(true).write(true).open(tmp)?;
            let mut file = MpegFile::read_from(&mut f, parse_opts()).map_err(lofty_err)?;
            let mut id3 = file.id3v2().cloned().unwrap_or_default();
            apply_id3(&mut id3, t, c, s);
            file.set_id3v2(id3);
            let mut opts = WriteOptions::default();
            let opts = opts.use_id3v23(true);
            rewind(&mut f)?;
            file.save_to(&mut f, opts).map_err(lofty_write_err)?;
            f.sync_all()?;
        }
        "flac" => {
            fs::copy(path, tmp)?;
            let mut f = OpenOptions::new().read(true).write(true).open(tmp)?;
            let mut file = FlacFile::read_from(&mut f, parse_opts()).map_err(lofty_err)?;
            let mut vc = file.vorbis_comments().cloned().unwrap_or_default();
            apply_vorbis(&mut vc, t, c, s);
            file.set_vorbis_comments(vc);
            apply_ogg_pictures(&mut file, c, s)?;
            rewind(&mut f)?;
            file.save_to(&mut f, WriteOptions::default()).map_err(lofty_write_err)?;
            f.sync_all()?;
        }
        "ogg" | "opus" => {
            fs::copy(path, tmp)?;
            let mut f = OpenOptions::new().read(true).write(true).open(tmp)?;
            if ext == "opus" {
                let mut file = OpusFile::read_from(&mut f, parse_opts()).map_err(lofty_err)?;
                let mut vc = file.vorbis_comments().clone();
                apply_vorbis(&mut vc, t, c, s);
                apply_ogg_pictures(&mut vc, c, s)?;
                file.set_vorbis_comments(vc);
                rewind(&mut f)?;
            file.save_to(&mut f, WriteOptions::default()).map_err(lofty_write_err)?;
            } else {
                let mut file = VorbisFile::read_from(&mut f, parse_opts()).map_err(lofty_err)?;
                let mut vc = file.vorbis_comments().clone();
                apply_vorbis(&mut vc, t, c, s);
                apply_ogg_pictures(&mut vc, c, s)?;
                file.set_vorbis_comments(vc);
                rewind(&mut f)?;
            file.save_to(&mut f, WriteOptions::default()).map_err(lofty_write_err)?;
            }
            f.sync_all()?;
        }
        "m4a" => {
            fs::copy(path, tmp)?;
            let mut f = OpenOptions::new().read(true).write(true).open(tmp)?;
            let mut file = Mp4File::read_from(&mut f, parse_opts()).map_err(lofty_err)?;
            let ilst = file.ilst().cloned().unwrap_or_default();
            // Découpage en tag générique + reste : les atomes inconnus sont conservés dans le reste.
            let (remainder, mut tag) = ilst.split_tag();
            let mut set = |k: ItemKey, v: Option<String>| {
                tag.remove_key(k.clone());
                if let Some(v) = v.filter(|v| !v.is_empty()) {
                    tag.insert_text(k, v);
                }
            };
            if let Some(title) = title_text(t, c, s, false) {
                set(ItemKey::TrackTitle, title);
            }
            if c.title.is_some() || c.version.is_some() {
                set(ItemKey::TrackSubtitle, t.version.clone());
            }
            if c.version.is_some() {
                set(ItemKey::Remixer, t.remixer.clone());
            }
            if c.artists.is_some() {
                set(ItemKey::TrackArtist, Some(join_artists(&t.artists, &s.artist_separator)));
            }
            if c.album.is_some() {
                set(ItemKey::AlbumTitle, t.album.clone());
            }
            if c.genre.is_some() {
                set(ItemKey::Genre, t.genre.clone());
            }
            if c.year.is_some() {
                set(ItemKey::RecordingDate, t.year.map(|y| y.to_string()));
            }
            if c.bpm.is_some() {
                set(ItemKey::IntegerBpm, t.bpm.map(|b| format!("{}", b.round() as i64)));
            }
            if c.key.is_some() {
                set(ItemKey::InitialKey, t.key.as_deref().map(|k| key_text(k, s)));
            }
            if let Some(cover) = &c.cover {
                tag.remove_picture_type(PictureType::CoverFront);
                if s.single_cover {
                    while !tag.pictures().is_empty() {
                        tag.remove_picture(0);
                    }
                }
                if let Some(cd) = cover {
                    tag.push_picture(make_picture(cd));
                }
            }
            let new_ilst = remainder.merge_tag(tag);
            file.set_ilst(new_ilst);
            rewind(&mut f)?;
            file.save_to(&mut f, WriteOptions::default()).map_err(lofty_write_err)?;
            f.sync_all()?;
        }
        _ => return Err(WriteError::Unsupported),
    }
    Ok(())
}

/// Compare les champs modifiés relus avec les valeurs voulues.
fn verify(read: &FileInfo, want: &TrackTags, c: &Changes, s: &WriteSettings) -> Result<(), String> {
    let r = &read.tags;
    let eq = |a: &Option<String>, b: &Option<String>| a.as_deref().map(str::trim) == b.as_deref().map(str::trim);
    if c.title.is_some() || c.version.is_some() {
        if !eq(&r.title, &want.title) {
            return Err(format!("titre relu {:?} au lieu de {:?}", r.title, want.title));
        }
        if !eq(&r.version, &want.version) {
            return Err(format!("version relue {:?} au lieu de {:?}", r.version, want.version));
        }
    }
    if let Some(raw) = &c.raw_title {
        if read.raw_title.as_deref().map(str::trim) != Some(raw.trim()) {
            return Err(format!("titre relu {:?} au lieu de {:?}", read.raw_title, raw));
        }
    }
    if c.artists.is_some() && r.artists != want.artists {
        return Err(format!("artistes relus {:?} au lieu de {:?}", r.artists, want.artists));
    }
    if c.album.is_some() && !eq(&r.album, &want.album) {
        return Err("album".into());
    }
    if c.genre.is_some() && !eq(&r.genre, &want.genre) {
        return Err("genre".into());
    }
    if c.year.is_some() && r.year != want.year {
        return Err(format!("année relue {:?} au lieu de {:?}", r.year, want.year));
    }
    if c.bpm.is_some() {
        let expect = want.bpm.map(|b| if s.bpm_integer { b.round() } else { (b * 100.0).round() / 100.0 });
        if r.bpm.map(|b| (b * 100.0).round() / 100.0) != expect {
            return Err(format!("BPM relu {:?} au lieu de {:?}", r.bpm, expect));
        }
    }
    if c.key.is_some() && r.key != want.key {
        return Err(format!("tonalité relue {:?} au lieu de {:?}", r.key, want.key));
    }
    if let Some(rt) = c.rating {
        // La note n'est pas gérée dans les fichiers M4A (voir docs/DECISIONS.md).
        if read.format != "m4a" && r.rating != rt {
            return Err(format!("note relue {} au lieu de {}", r.rating, rt));
        }
    }
    if let Some(cover) = &c.cover {
        let has = read.cover.count > 0;
        if has != cover.is_some() {
            return Err("pochette".into());
        }
    }
    // WAV : après une resynchronisation, RIFF INFO et ID3 doivent dire la même chose.
    if c.sync_wav && read.wav_desync {
        return Err("RIFF INFO et ID3 différents".into());
    }
    Ok(())
}

/// Résultat d'une écriture réussie : informations relues dans le fichier final.
pub struct Written {
    pub info: FileInfo,
    pub size: u64,
    pub mtime: i64,
}

/// Écrit `changes` dans le fichier `path`, en toute sécurité.
/// `current` : tags actuels du fichier (lus juste avant), base des valeurs complètes.
pub fn write_tags(
    path: &Path,
    current: &TrackTags,
    changes: &Changes,
    settings: &WriteSettings,
    exceptions: &[String],
) -> Result<Written, WriteError> {
    let meta = fs::metadata(path)?;
    if meta.permissions().readonly() {
        return Err(WriteError::ReadOnly);
    }
    let want = merged(current, changes);
    let hash_before = audio_stream_hash(path, None).map_err(|e| WriteError::Verify(e.to_string()))?;

    let tmp = temp_path(path);
    let guard = TempGuard(tmp.clone());
    write_into_temp(path, &tmp, &want, changes, settings)?;

    // Relecture de contrôle.
    let read = read_file(&tmp, exceptions).map_err(|e| WriteError::Verify(e.to_string()))?;
    verify(&read, &want, changes, settings).map_err(WriteError::Verify)?;
    let hash_after = audio_stream_hash(&tmp, None).map_err(|e| WriteError::Verify(e.to_string()))?;
    if hash_after != hash_before {
        return Err(WriteError::AudioChanged);
    }

    // Remplacement atomique (rename sur le même volume).
    fs::rename(&tmp, path).map_err(classify_io)?;
    std::mem::forget(guard); // le fichier temporaire est devenu l'original
    // Assure que le dossier enregistre le remplacement.
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    let meta = fs::metadata(path)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(Written { info: read, size: meta.len(), mtime })
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
