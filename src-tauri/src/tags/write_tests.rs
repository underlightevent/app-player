//! Tests de l'écriture sûre : aller-retour de chaque champ, tags inconnus préservés,
//! flux audio identique, WAV synchronisés, échecs sans dommage.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::AudioFile;
use lofty::id3::v2::{
    BinaryFrame, CommentFrame, ExtendedTextFrame, Frame, FrameId, Id3v2Tag, PopularimeterFrame, PrivateFrame,
};
use lofty::tag::{Accessor, TagExt};
use lofty::TextEncoding;

use super::*;
use crate::artwork;
use crate::audio::hash::audio_stream_hash;
use crate::tags::chunks;
use crate::tags::read::{read_file, read_riff_info};
use crate::testdata;

const GEOB_DATA: &[u8] = b"\0application/octet-stream\0\0Serato Markers2\0SERATO-DONNEES-BINAIRES-\x01\x02\x03";

fn geob() -> Frame<'static> {
    Frame::Binary(BinaryFrame::new(FrameId::Valid(Cow::Borrowed("GEOB")), GEOB_DATA.to_vec()))
}

/// Tag ID3 « d'un autre logiciel » : données Serato, Rekordbox, Beatport, commentaire, note tierce.
fn foreign_id3() -> Id3v2Tag {
    let mut t = Id3v2Tag::new();
    t.insert(geob());
    t.insert(Frame::Private(PrivateFrame::new("rekordbox", vec![9u8, 8, 7, 6])));
    t.insert(Frame::UserText(ExtendedTextFrame::new(
        TextEncoding::UTF16,
        "BEATPORT_TRACK_ID".to_string(),
        "123456".to_string(),
    )));
    t.insert(Frame::Comment(CommentFrame::new(TextEncoding::UTF16, *b"eng", String::new(), "Commentaire d'origine".to_string())));
    t.insert(Frame::Popularimeter(PopularimeterFrame::new("traktor@native-instruments.de", 200, 3)));
    t
}

fn has_foreign_frames(tag: &Id3v2Tag) {
    let geob = tag.iter().find(|f| f.id_str() == "GEOB").expect("GEOB (Serato) perdu");
    match geob {
        Frame::Binary(b) => assert_eq!(&*b.data, GEOB_DATA, "données Serato modifiées"),
        other => panic!("GEOB inattendu : {other:?}"),
    }
    assert!(tag.iter().any(|f| matches!(f, Frame::Private(p) if p.owner == "rekordbox" && *p.private_data == [9u8, 8, 7, 6])), "PRIV (Rekordbox) perdu");
    assert_eq!(tag.get_user_text("BEATPORT_TRACK_ID"), Some("123456"), "TXXX (Beatport) perdu");
    assert!(tag.iter().any(|f| matches!(f, Frame::Comment(c) if c.content == "Commentaire d'origine")), "commentaire perdu");
    assert!(
        tag.iter().any(|f| matches!(f, Frame::Popularimeter(p) if p.email == "traktor@native-instruments.de" && p.rating == 200)),
        "note d'un autre logiciel perdue"
    );
}

fn read_id3(path: &Path) -> Id3v2Tag {
    let mut f = std::fs::File::open(path).unwrap();
    let ext = path.extension().unwrap().to_str().unwrap();
    let opts = ParseOptions::new();
    match ext {
        "mp3" => lofty::mpeg::MpegFile::read_from(&mut f, opts).unwrap().id3v2().cloned().unwrap(),
        "wav" => lofty::iff::wav::WavFile::read_from(&mut f, opts).unwrap().id3v2().cloned().unwrap(),
        "aiff" => lofty::iff::aiff::AiffFile::read_from(&mut f, opts).unwrap().id3v2().cloned().unwrap(),
        _ => unreachable!(),
    }
}

/// WAV avec bloc bext, RIFF INFO (dont un texte Windows-1252) et ID3 étranger.
fn foreign_wav(dir: &Path) -> PathBuf {
    let mut bytes = testdata::wav_bytes(5);
    let info = chunks::build_info(&[
        chunks::InfoItem { id: *b"ISFT", raw: b"Logiciel \xe9tranger".to_vec() },
        chunks::InfoItem { id: *b"ICMT", raw: b"Commentaire RIFF".to_vec() },
    ]);
    for (id, data) in [(*b"bext", vec![0x42u8; 602]), (*b"LIST", info)] {
        bytes.extend(id);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(&data);
        if data.len() % 2 == 1 {
            bytes.push(0);
        }
    }
    let total = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&total.to_le_bytes());
    let p = testdata::write(dir, "etranger.wav", &bytes);
    let mut opts = WriteOptions::default();
    foreign_id3().save_to_path(&p, opts.use_id3v23(true)).unwrap();
    p
}

fn full_changes(cover: Option<CoverData>) -> Changes {
    Changes {
        title: Some(Some("Été Indien".into())),
        version: Some(Some("DJ Lumière Remix".into())),
        artists: Some(vec!["Artiste A".into(), "Zoë B".into(), "Ñandú C".into()]),
        album: Some(Some("Album Spécial".into())),
        year: Some(Some(2024)),
        genre: Some(Some("Mélodique Techno".into())),
        bpm: Some(Some(123.6)),
        key: Some(Some("F#m".into())),
        rating: Some(4),
        cover: Some(cover),
        sync_wav: false,
        raw_title: None,
    }
    .normalized()
}

fn check_full(path: &Path, cover_expected: bool) {
    let info = read_file(path, &[]).unwrap();
    let t = &info.tags;
    assert_eq!(t.title.as_deref(), Some("Été Indien"), "{}", path.display());
    assert_eq!(t.version.as_deref(), Some("DJ Lumière Remix"));
    assert_eq!(t.artists, ["Artiste A", "Zoë B", "Ñandú C"]);
    assert_eq!(t.album.as_deref(), Some("Album Spécial"));
    assert_eq!(t.year, Some(2024));
    assert_eq!(t.genre.as_deref(), Some("Mélodique Techno"));
    assert_eq!(t.bpm, Some(124.0)); // arrondi à l'entier par défaut
    assert_eq!(t.key.as_deref(), Some("F#m"));
    assert_eq!(t.rating, 4);
    assert_eq!(t.remixer.as_deref(), Some("DJ Lumière"));
    assert_eq!(info.cover.count, if cover_expected { 1 } else { 0 });
    assert!(!info.wav_desync, "WAV désynchronisé après écriture");
}

fn cover() -> CoverData {
    artwork::prepare(&artwork::test_png(800, 800), 1000).unwrap().cover
}

fn roundtrip(path: &Path) {
    let before_hash = audio_stream_hash(path, None).unwrap();
    let current = read_file(path, &[]).unwrap().tags;
    let s = WriteSettings::default();
    write_tags(path, &current, &full_changes(Some(cover())), &s, &[]).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    check_full(path, true);
    assert_eq!(audio_stream_hash(path, None).unwrap(), before_hash, "flux audio modifié : {}", path.display());

    // Deuxième écriture : seulement la note. Le reste ne bouge pas.
    let current = read_file(path, &[]).unwrap().tags;
    let c = Changes { rating: Some(2), ..Default::default() };
    write_tags(path, &current, &c, &s, &[]).unwrap();
    let after = read_file(path, &[]).unwrap();
    assert_eq!(after.tags.rating, 2);
    assert_eq!(after.tags.title.as_deref(), Some("Été Indien"));
    assert_eq!(after.cover.count, 1);
    assert_eq!(audio_stream_hash(path, None).unwrap(), before_hash);

    // Vider des champs et retirer la pochette.
    let current = after.tags;
    let c = Changes { album: Some(None), cover: Some(None), version: Some(None), ..Default::default() };
    write_tags(path, &current, &c, &s, &[]).unwrap();
    let after = read_file(path, &[]).unwrap();
    assert_eq!(after.tags.album, None);
    assert_eq!(after.tags.version, None);
    assert_eq!(after.tags.title.as_deref(), Some("Été Indien"));
    assert_eq!(after.tags.remixer, None, "remixeur resté après suppression de la version");
    assert_eq!(after.cover.count, 0);
    assert_eq!(audio_stream_hash(path, None).unwrap(), before_hash);
    // Aucun fichier temporaire laissé.
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".djlm-tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

/// MP3 réel de l'utilisateur : tag APE de fin abîmé (annonce des éléments, ne contient
/// que des zéros) devant un ID3v1. Les lecteurs l'ignorent ; il ne doit plus rendre le
/// fichier « illisible », et l'écriture des tags ne doit pas toucher à la fin du fichier.
#[test]
fn mp3_avec_tag_ape_abime() {
    let dir = tempfile::tempdir().unwrap();
    let path = testdata::write(dir.path(), "ape.mp3", &testdata::mp3_bytes());
    let mut tag = foreign_id3();
    tag.set_title("On avance (Original)".into());
    tag.set_artist("Alain Souchon".into());
    tag.save_to_path(&path, WriteOptions::default().use_id3v23(true)).unwrap();
    // Tag APE abîmé + ID3v1, comme dans les fichiers de l'utilisateur.
    let mut tail = Vec::new();
    tail.extend_from_slice(&[0u8; 142]);
    tail.extend_from_slice(b"APETAGEX");
    for v in [2000u32, 174, 4, 0xa000_0000] {
        tail.extend_from_slice(&v.to_le_bytes());
    }
    tail.extend_from_slice(&[0u8; 8]);
    let mut v1 = b"TAG".to_vec();
    v1.resize(128, b' ');
    tail.extend_from_slice(&v1);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&tail);
    std::fs::write(&path, &bytes).unwrap();
    assert!(lofty::read_from_path(&path).is_err(), "le cas doit reproduire l'erreur d'origine");

    let info = read_file(&path, &[]).expect("lisible malgré le tag APE abîmé");
    assert_eq!(info.tags.title.as_deref(), Some("On avance"));
    assert_eq!(info.tags.version.as_deref(), Some("Original"));
    assert_eq!(info.tags.artists, ["Alain Souchon"]);
    assert!(info.duration_ms > 0);

    let hash = audio_stream_hash(&path, None).unwrap();
    let c = Changes { title: Some(Some("Nouveau".into())), rating: Some(4), ..Default::default() };
    write_tags(&path, &info.tags, &c, &WriteSettings::default(), &[]).expect("écriture possible");
    let after = read_file(&path, &[]).unwrap();
    assert_eq!(after.tags.title.as_deref(), Some("Nouveau"));
    assert_eq!(after.tags.rating, 4);
    assert_eq!(audio_stream_hash(&path, None).unwrap(), hash, "son identique");
    let head = crate::tags::read::salvage_bytes(&path, false).unwrap();
    let id3 = MpegFile::read_from(&mut std::io::Cursor::new(head), ParseOptions::new().read_properties(false)).unwrap();
    has_foreign_frames(id3.id3v2().unwrap());
    let new_bytes = std::fs::read(&path).unwrap();
    assert!(new_bytes.ends_with(&tail), "fin du fichier (APE, ID3v1) intacte");
}

#[test]
fn json_distingue_inchange_et_vide() {
    let c = Changes { title: Some(None), album: Some(Some("A".into())), ..Default::default() };
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(json, r#"{"title":null,"album":"A"}"#);
    let back: Changes = serde_json::from_str(&json).unwrap();
    assert_eq!(back.title, Some(None));
    assert_eq!(back.version, None);
    assert_eq!(back.album, Some(Some("A".into())));
}

#[test]
fn aller_retour_chaque_format() {
    let dir = tempfile::tempdir().unwrap();
    for p in testdata::all_formats(dir.path()) {
        roundtrip(&p);
    }
}

#[test]
fn wav_tags_inconnus_et_riff_info_preserves() {
    let dir = tempfile::tempdir().unwrap();
    let p = foreign_wav(dir.path());
    let bext_before = {
        let mut f = std::fs::File::open(&p).unwrap();
        let l = chunks::read_layout(&mut f).unwrap();
        chunks::read_chunk(&mut f, l.chunks.iter().find(|c| c.is(b"bext")).unwrap()).unwrap()
    };
    roundtrip(&p);
    has_foreign_frames(&read_id3(&p));
    let info = read_riff_info(&p).unwrap();
    let isft = info.iter().find(|i| &i.id == b"ISFT").expect("ISFT perdu");
    assert_eq!(isft.raw, b"Logiciel \xe9tranger", "texte Windows-1252 modifié");
    assert!(info.iter().any(|i| &i.id == b"ICMT" && i.raw == b"Commentaire RIFF"));
    let inam = info.iter().find(|i| &i.id == b"INAM").expect("INAM absent");
    assert_eq!(chunks::decode_text(&inam.raw), "Été Indien");
    let mut f = std::fs::File::open(&p).unwrap();
    let l = chunks::read_layout(&mut f).unwrap();
    assert_eq!(chunks::read_chunk(&mut f, l.chunks.iter().find(|c| c.is(b"bext")).unwrap()).unwrap(), bext_before);
    assert_eq!(l.chunks.iter().filter(|c| c.is_id3()).count(), 1, "un seul bloc ID3");
}

#[test]
fn wav_riff_info_et_id3_identiques() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "a.wav", &testdata::wav_bytes(1));
    let current = read_file(&p, &[]).unwrap().tags;
    write_tags(&p, &current, &full_changes(None), &WriteSettings::default(), &[]).unwrap();
    let info = read_riff_info(&p).unwrap();
    let get = |id: &[u8; 4]| chunks::decode_text(&info.iter().find(|i| &i.id == id).unwrap().raw);
    assert_eq!(get(b"INAM"), "Été Indien (DJ Lumière Remix)");
    assert_eq!(get(b"IART"), "Artiste A, Zoë B, Ñandú C");
    assert_eq!(get(b"IPRD"), "Album Spécial");
    assert_eq!(get(b"IGNR"), "Mélodique Techno");
    assert_eq!(get(b"ICRD"), "2024");
    assert!(!read_file(&p, &[]).unwrap().wav_desync);
}

#[test]
fn mp3_et_aiff_tags_inconnus_preserves() {
    let dir = tempfile::tempdir().unwrap();
    for (name, bytes) in [("x.mp3", testdata::mp3_bytes()), ("x.aiff", testdata::aiff_bytes(4))] {
        let p = testdata::write(dir.path(), name, &bytes);
        let mut opts = WriteOptions::default();
        foreign_id3().save_to_path(&p, opts.use_id3v23(true)).unwrap();
        roundtrip(&p);
        has_foreign_frames(&read_id3(&p));
    }
}

#[test]
fn mp3_ecrit_en_id3v23_et_titre_compose() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "v.mp3", &testdata::mp3_bytes());
    let current = read_file(&p, &[]).unwrap().tags;
    write_tags(&p, &current, &full_changes(None), &WriteSettings::default(), &[]).unwrap();
    let bytes = std::fs::read(&p).unwrap();
    assert_eq!(&bytes[0..4], b"ID3\x03", "ID3v2.3 attendu");
    let tag = read_id3(&p);
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TIT2"))), Some("Été Indien (DJ Lumière Remix)"));
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TIT3"))), Some("DJ Lumière Remix"));
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TPE4"))), Some("DJ Lumière"));
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TKEY"))), Some("11A"));
    assert!(tag.iter().any(|f| matches!(f, Frame::Popularimeter(p) if p.email == POPM_EMAIL && p.rating == 196)));
}

#[test]
fn flac_commentaires_inconnus_preserves() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "s.flac", &testdata::flac_bytes(3));
    let mut vc = lofty::ogg::tag::VorbisComments::new();
    vc.insert("SERATO_MARKERS_V2".into(), "QVBQRGF0YQ==".into());
    vc.insert("COMMENT".into(), "Commentaire".into());
    vc.save_to_path(&p, WriteOptions::default()).unwrap();
    roundtrip(&p);
    let mut f = std::fs::File::open(&p).unwrap();
    let flac = lofty::flac::FlacFile::read_from(&mut f, ParseOptions::new()).unwrap();
    let vc = flac.vorbis_comments().unwrap();
    assert_eq!(vc.get("SERATO_MARKERS_V2"), Some("QVBQRGF0YQ=="));
    assert_eq!(vc.get("COMMENT"), Some("Commentaire"));
}

#[test]
fn ecriture_pendant_la_lecture() {
    // Le lecteur garde le fichier ouvert : l'écriture doit réussir sans couper la lecture.
    let dir = tempfile::tempdir().unwrap();
    for p in testdata::all_formats(dir.path()) {
        let mut playing = crate::audio::decode::Decoder::open(&p).unwrap();
        let mut buf = Vec::new();
        assert!(playing.next_block(&mut buf).unwrap());
        let current = read_file(&p, &[]).unwrap().tags;
        let c = Changes { title: Some(Some("Pendant la lecture".into())), ..Default::default() };
        write_tags(&p, &current, &c, &WriteSettings::default(), &[]).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(read_file(&p, &[]).unwrap().tags.title.as_deref(), Some("Pendant la lecture"));
        // La lecture continue sur l'ancienne version du fichier.
        while playing.next_block(&mut buf).unwrap() {}
    }
}

#[test]
fn fichier_en_lecture_seule_intact() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "ro.mp3", &testdata::mp3_bytes());
    let before = std::fs::read(&p).unwrap();
    let mut perm = std::fs::metadata(&p).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&p, perm.clone()).unwrap();
    let current = read_file(&p, &[]).unwrap().tags;
    let r = write_tags(&p, &current, &full_changes(None), &WriteSettings::default(), &[]);
    assert!(matches!(r, Err(WriteError::ReadOnly)), "{:?}", r.err());
    assert_eq!(std::fs::read(&p).unwrap(), before);
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&p, perm).unwrap();
}

#[test]
fn fichier_corrompu_refuse_sans_dommage() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "casse.flac", b"fLaC\x00\x00\x00\x22tronque");
    let before = std::fs::read(&p).unwrap();
    let r = write_tags(&p, &TrackTags::default(), &full_changes(None), &WriteSettings::default(), &[]);
    assert!(r.is_err());
    assert_eq!(std::fs::read(&p).unwrap(), before);
}

#[test]
fn reglages_titre_seul_et_separateur() {
    let dir = tempfile::tempdir().unwrap();
    let p = testdata::write(dir.path(), "r.mp3", &testdata::mp3_bytes());
    let s = WriteSettings { version_in_title: false, artist_separator: "&".into(), key_camelot: false, ..Default::default() };
    let current = read_file(&p, &[]).unwrap().tags;
    write_tags(&p, &current, &full_changes(None), &s, &[]).unwrap();
    let tag = read_id3(&p);
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TIT2"))), Some("Été Indien"));
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TPE1"))), Some("Artiste A & Zoë B & Ñandú C"));
    assert_eq!(tag.get_text(&FrameId::Valid(Cow::Borrowed("TKEY"))), Some("F#m"));
    let info = read_file(&p, &[]).unwrap();
    assert_eq!(info.tags.version.as_deref(), Some("DJ Lumière Remix")); // relue depuis TIT3
}

/// Diagnostic sur des COPIES des fichiers d'un vrai dossier (les originaux sont seulement lus) :
/// change le titre de chaque copie et liste les échecs.
/// `DJLM_DIAG_DIR=E:\... DJLM_DIAG_STEP=20 cargo test diag_ecriture -- --ignored --nocapture`
#[test]
#[ignore]
fn diag_ecriture() {
    let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
    let step: usize = std::env::var("DJLM_DIAG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let tmp = tempfile::tempdir().unwrap();
    let (mut ok, mut bad) = (0, 0);
    for (i, e) in std::fs::read_dir(dir).unwrap().flatten().enumerate() {
        let p = e.path();
        if i % step != 0 || !crate::tags::is_audio_path(&p) {
            continue;
        }
        let copy = tmp.path().join(format!("copie.{}", p.extension().unwrap().to_string_lossy()));
        std::fs::copy(&p, &copy).unwrap();
        let res = read_file(&copy, &[]).map_err(|e| format!("lecture: {e}")).and_then(|info| {
            let new_title = format!("{} X", info.tags.title.clone().unwrap_or_default());
            let c = Changes { title: Some(Some(new_title)), ..Default::default() };
            write_tags(&copy, &info.tags, &c, &WriteSettings::default(), &[]).map(|_| ()).map_err(|e| format!("{e:?}"))
        });
        match res {
            Ok(()) => ok += 1,
            Err(e) => {
                bad += 1;
                println!("ECHEC|{}|{}", p.file_name().unwrap().to_string_lossy(), e);
            }
        }
        let _ = std::fs::remove_file(&copy);
    }
    println!("ECRITURE ok={ok} echecs={bad}");
}

/// Cas réel : titre saisi « Artiste - Titre » ou « Titre (Extended) » sans version.
/// La relecture sépare l'artiste et la version : l'écriture ne doit pas être refusée.
#[test]
fn titre_avec_artiste_ou_version_saisi_dans_le_titre() {
    let dir = tempfile::tempdir().unwrap();
    for (title, expect_title, expect_version) in [
        ("113 - Au Summum", "Au Summum", None),
        ("113 - 113 - Au Summum", "Au Summum", None),
        ("La Boulette (Extended)", "La Boulette", Some("Extended")),
    ] {
        let path = testdata::write(dir.path(), "t.mp3", &testdata::mp3_bytes());
        let info = read_file(&path, &[]).unwrap();
        let c = Changes { title: Some(Some(title.into())), artists: Some(vec!["113".into()]), ..Default::default() };
        write_tags(&path, &info.tags, &c, &WriteSettings::default(), &[]).unwrap_or_else(|e| panic!("{title}: {e:?}"));
        let after = read_file(&path, &[]).unwrap();
        assert_eq!(after.raw_title.as_deref(), Some(title), "texte écrit tel quel");
        assert_eq!(after.tags.title.as_deref(), Some(expect_title));
        assert_eq!(after.tags.version.as_deref(), expect_version);
    }
}
