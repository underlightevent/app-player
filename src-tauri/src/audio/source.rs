//! Ouverture d'un fichier pour Symphonia, en sautant nous-mêmes le tag ID3v2 du début.
//!
//! Symphonia ne cherche le début du son que dans le premier Mo : un MP3 dont le tag ID3v2
//! dépasse 1 Mo (grandes pochettes, données de logiciels DJ) est refusé. Et en parcourant
//! le tag, il peut prendre des octets de la pochette pour un autre format (AAC) et se
//! tromper de décodeur. Les paquets audio lus sont les mêmes : seul le tag est sauté.
//! Lecture seule : le fichier n'est jamais modifié.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::formats::probe::Hint;
use symphonia::core::io::{MediaSource, MediaSourceStream};

/// Fichier vu à partir de l'octet `start` (le tag ID3v2 est invisible).
struct Skipped {
    f: File,
    start: u64,
    len: u64,
}

impl Read for Skipped {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.f.read(buf)
    }
}

impl Seek for Skipped {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => SeekFrom::Start(self.start + n),
            other => other,
        };
        let abs = self.f.seek(target)?;
        if abs < self.start {
            self.f.seek(SeekFrom::Start(self.start))?;
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "avant le début du son"));
        }
        Ok(abs - self.start)
    }
}

impl MediaSource for Skipped {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len - self.start)
    }
}

/// Taille d'un en-tête ID3v2 valide (en-tête et pied compris), ou `None`.
fn tag_size(head: &[u8]) -> Option<u64> {
    if head.len() < 10 || &head[..3] != b"ID3" || !(2..=4).contains(&head[3]) || head[4] != 0 || head[6..10].iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    let size = head[6..10].iter().fold(0u64, |a, b| (a << 7) | *b as u64);
    Some(10 + size + if head[5] & 0x10 != 0 { 10 } else { 0 })
}

/// Fin du ou des tags ID3v2 du début du fichier (0 s'il n'y en a pas). Certains logiciels
/// ajoutent un second tag juste après le premier, parfois après quelques octets (taille du
/// premier mal calculée) : on le saute aussi s'il commence dans les 4 Kio qui suivent.
pub fn id3v2_end(f: &mut File) -> io::Result<u64> {
    let mut head = [0u8; 10];
    f.seek(SeekFrom::Start(0))?;
    let n = f.read(&mut head)?;
    let Some(mut end) = tag_size(&head[..n]) else {
        f.seek(SeekFrom::Start(0))?;
        return Ok(0);
    };
    loop {
        let mut window = vec![0u8; 4096 + 10];
        f.seek(SeekFrom::Start(end))?;
        let n = f.read(&mut window)?;
        let next = (0..n.saturating_sub(9)).find_map(|i| tag_size(&window[i..n]).map(|s| (i as u64, s)));
        match next {
            Some((offset, size)) => end += offset + size,
            None => break,
        }
    }
    f.seek(SeekFrom::Start(0))?;
    Ok(end)
}

/// Flux en mémoire (extrait téléchargé pour l'écoute, jamais écrit sur le disque).
pub fn open_bytes(bytes: Vec<u8>, ext: &str) -> (MediaSourceStream<'static>, Hint) {
    let mut hint = Hint::new();
    hint.with_extension(ext);
    (MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), Default::default()), hint)
}

/// Flux prêt pour la sonde de Symphonia, avec l'indice d'extension.
pub fn open(path: &Path) -> io::Result<(MediaSourceStream<'static>, Hint)> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let start = id3v2_end(&mut f)?;
    let source: Box<dyn MediaSource> = if start > 0 && start < len {
        f.seek(SeekFrom::Start(start))?;
        Box::new(Skipped { f, start, len })
    } else {
        Box::new(f)
    };
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    Ok((MediaSourceStream::new(source, Default::default()), hint))
}

#[cfg(test)]
mod tests {
    /// Tag ID3v2.3 avec une trame PRIV de `size` octets qui imitent un en-tête AAC (ADTS).
    fn id3_tag(size: usize) -> Vec<u8> {
        let mut data = b"djlm\0".to_vec();
        while data.len() < size {
            data.extend_from_slice(&[0xFF, 0xF1, 0x50, 0x80, 0x02, 0x1F, 0xFC]);
        }
        let mut frame = b"PRIV".to_vec();
        frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
        frame.extend_from_slice(&[0, 0]);
        frame.extend_from_slice(&data);
        let n = frame.len() as u32;
        let mut tag = b"ID3\x03\0\0".to_vec();
        tag.extend_from_slice(&[(n >> 21) as u8 & 0x7f, (n >> 14) as u8 & 0x7f, (n >> 7) as u8 & 0x7f, n as u8 & 0x7f]);
        tag.extend_from_slice(&frame);
        tag
    }

    #[test]
    fn mp3_avec_tag_enorme_ou_double() {
        let dir = tempfile::tempdir().unwrap();
        let audio = crate::testdata::mp3_bytes();
        let plain = crate::testdata::write(dir.path(), "plain.mp3", &audio);
        let reference = crate::audio::hash::audio_stream_hash(&plain, None).unwrap();

        // Tag de 1,5 Mo (au-delà de ce que Symphonia parcourt) rempli de faux en-têtes AAC.
        let mut big = id3_tag(1_500_000);
        big.extend_from_slice(&audio);
        let p = crate::testdata::write(dir.path(), "gros_tag.mp3", &big);
        assert_eq!(crate::audio::hash::audio_stream_hash(&p, None).unwrap(), reference);
        assert!(crate::audio::decode::Decoder::open(&p).is_ok());

        // Deux tags à la suite, le premier annonçant une taille trop courte de 8 octets.
        let mut first = id3_tag(5000);
        first.extend_from_slice(&[0xAB; 8]);
        let mut double = first;
        double.extend_from_slice(&id3_tag(300));
        double.extend_from_slice(&audio);
        let p = crate::testdata::write(dir.path(), "double.mp3", &double);
        assert_eq!(crate::audio::hash::audio_stream_hash(&p, None).unwrap(), reference);
        assert!(crate::audio::decode::Decoder::open(&p).is_ok());
    }

    /// Diagnostic sur un vrai dossier (lecture seule) : l'empreinte des fichiers que l'ancienne
    /// ouverture savait lire ne doit pas changer.
    /// `DJLM_DIAG_DIR=E:\... cargo test empreintes_inchangees -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn empreintes_inchangees() {
        use symphonia::core::formats::{FormatOptions, TrackType};
        use symphonia::core::meta::MetadataOptions;
        let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
        let old_hash = |p: &std::path::Path| -> Option<String> {
            let f = std::fs::File::open(p).ok()?;
            let mss = symphonia::core::io::MediaSourceStream::new(Box::new(f), Default::default());
            let mut hint = symphonia::core::formats::probe::Hint::new();
            hint.with_extension(p.extension()?.to_str()?);
            let mut fmt = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default()).ok()?;
            let id = fmt.default_track(TrackType::Audio)?.id;
            let mut h = blake3::Hasher::new();
            while let Ok(Some(pk)) = fmt.next_packet() {
                if pk.track_id == id {
                    h.update(&pk.data);
                }
            }
            Some(h.finalize().to_hex().to_string())
        };
        let (mut same, mut diff, mut fixed) = (0, 0, 0);
        let step: usize = std::env::var("DJLM_DIAG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
        for (i, e) in std::fs::read_dir(dir).unwrap().flatten().enumerate() {
            let p = e.path();
            if i % step != 0 || !crate::tags::is_audio_path(&p) {
                continue;
            }
            let new = crate::audio::hash::audio_stream_hash(&p, None).ok();
            match (old_hash(&p), new) {
                (Some(o), Some(n)) if o == n => same += 1,
                (Some(_), Some(_)) => {
                    diff += 1;
                    println!("DIFF {}", p.display());
                }
                _ => fixed += 1,
            }
        }
        println!("EMPREINTES identiques={same} differentes={diff} autres={fixed}");
    }
}
