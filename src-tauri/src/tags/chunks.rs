//! Lecture et réécriture des fichiers WAV (RIFF) et AIFF (FORM) au niveau des blocs.
//!
//! Pourquoi ne pas tout confier à lofty : lofty ignore les entrées RIFF INFO qui ne
//! sont pas en UTF-8 (fréquent sous Windows). Les réécrire ferait perdre des données.
//! Ici, chaque bloc est recopié octet pour octet ; seuls les blocs de tags
//! (LIST/INFO et id3) sont remplacés. Le bloc audio n'est jamais touché.

use std::io::{self, Read, Seek, SeekFrom, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Wav,
    Aiff,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: [u8; 4],
    /// Position des données (après l'en-tête de 8 octets).
    pub data_offset: u64,
    pub size: u32,
}

impl Chunk {
    pub fn is(&self, id: &[u8; 4]) -> bool {
        &self.id == id
    }
    pub fn is_id3(&self) -> bool {
        self.id.eq_ignore_ascii_case(b"id3 ")
    }
}

#[derive(Debug)]
pub struct Layout {
    pub container: Container,
    pub form_type: [u8; 4],
    pub chunks: Vec<Chunk>,
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Liste des blocs d'un fichier WAV ou AIFF.
pub fn read_layout<R: Read + Seek>(r: &mut R) -> io::Result<Layout> {
    r.seek(SeekFrom::Start(0))?;
    let mut head = [0u8; 12];
    r.read_exact(&mut head)?;
    let (container, le) = match &head[0..4] {
        b"RIFF" => (Container::Wav, true),
        b"FORM" => (Container::Aiff, false),
        b"RF64" => return Err(invalid("WAV RF64 (plus de 4 Go) non pris en charge pour l'écriture")),
        _ => return Err(invalid("en-tête WAV/AIFF introuvable")),
    };
    let form_type: [u8; 4] = head[8..12].try_into().unwrap();
    let file_len = r.seek(SeekFrom::End(0))?;
    let mut pos = 12u64;
    let mut chunks = Vec::new();
    while pos + 8 <= file_len {
        r.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 8];
        r.read_exact(&mut h)?;
        let id: [u8; 4] = h[0..4].try_into().unwrap();
        let raw = [h[4], h[5], h[6], h[7]];
        let size = if le { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) };
        let data_offset = pos + 8;
        // Bloc tronqué : on le garde tel quel jusqu'à la fin du fichier (fichiers mal fermés).
        let available = file_len.saturating_sub(data_offset);
        let size = if size as u64 > available { available as u32 } else { size };
        chunks.push(Chunk { id, data_offset, size });
        pos = data_offset + size as u64 + (size as u64 & 1);
    }
    Ok(Layout { container, form_type, chunks })
}

/// Lit les données d'un bloc.
pub fn read_chunk<R: Read + Seek>(r: &mut R, c: &Chunk) -> io::Result<Vec<u8>> {
    r.seek(SeekFrom::Start(c.data_offset))?;
    let mut buf = vec![0u8; c.size as usize];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

// ---------- RIFF INFO ----------

/// Entrée brute d'un bloc INFO : identifiant et octets (sans zéros de fin).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoItem {
    pub id: [u8; 4],
    pub raw: Vec<u8>,
}

/// Le bloc `LIST` de type `INFO`, s'il existe.
pub fn find_info(layout: &Layout, r: &mut (impl Read + Seek)) -> io::Result<Option<(usize, Vec<InfoItem>)>> {
    for (i, c) in layout.chunks.iter().enumerate() {
        if c.is(b"LIST") && c.size >= 4 {
            let data = read_chunk(r, c)?;
            if &data[0..4] == b"INFO" {
                return Ok(Some((i, parse_info(&data[4..]))));
            }
        }
    }
    Ok(None)
}

pub fn parse_info(mut data: &[u8]) -> Vec<InfoItem> {
    let mut items = Vec::new();
    while data.len() >= 8 {
        let id: [u8; 4] = data[0..4].try_into().unwrap();
        let size = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
        let end = (8 + size).min(data.len());
        let mut raw = data[8..end].to_vec();
        while raw.last() == Some(&0) {
            raw.pop();
        }
        items.push(InfoItem { id, raw });
        let next = 8 + size + (size & 1);
        if next > data.len() {
            break;
        }
        data = &data[next..];
    }
    items
}

pub fn build_info(items: &[InfoItem]) -> Vec<u8> {
    let mut out = b"INFO".to_vec();
    for it in items {
        if it.raw.is_empty() {
            continue;
        }
        let len = it.raw.len() + 1; // zéro final
        out.extend_from_slice(&it.id);
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&it.raw);
        out.push(0);
        if len & 1 == 1 {
            out.push(0);
        }
    }
    out
}

/// Décodage tolérant : UTF-8 si valide, sinon Windows-1252.
pub fn decode_text(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(s) => s.to_string(),
        Err(_) => raw.iter().map(|b| cp1252(*b)).collect(),
    }
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9f => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

// ---------- Réécriture ----------

/// Contenu à mettre à la place des blocs de tags.
pub struct Replacement<'a> {
    /// Données du bloc LIST (commençant par « INFO »), ou None pour ne pas y toucher.
    pub info: Option<&'a [u8]>,
    /// Tag ID3 complet (en-tête compris), ou None pour ne pas y toucher.
    pub id3: Option<&'a [u8]>,
}

/// Écrit dans `out` une copie de `src` où seuls les blocs de tags sont remplacés.
/// Les blocs remplacés gardent leur place ; les nouveaux sont ajoutés à la fin.
pub fn rewrite<R: Read + Seek, W: Write>(src: &mut R, layout: &Layout, rep: &Replacement, out: &mut W) -> io::Result<()> {
    let le = layout.container == Container::Wav;
    let id3_id: &[u8; 4] = if le { b"id3 " } else { b"ID3 " };
    let mut body: Vec<u8> = Vec::new(); // seulement les petits blocs ; l'audio est copié en flux
    let mut parts: Vec<Part> = Vec::new();
    let mut info_done = rep.info.is_none();
    let mut id3_done = rep.id3.is_none();
    let info_index = if rep.info.is_some() { find_info_index(layout, src)? } else { None };

    for (i, c) in layout.chunks.iter().enumerate() {
        if Some(i) == info_index {
            if !info_done {
                parts.push(Part::New(*b"LIST", rep.info.unwrap().to_vec()));
                info_done = true;
            }
            continue;
        }
        if c.is_id3() && rep.id3.is_some() {
            if !id3_done {
                parts.push(Part::New(*id3_id, rep.id3.unwrap().to_vec()));
                id3_done = true;
            }
            continue; // doublons éventuels supprimés : un seul bloc ID3
        }
        parts.push(Part::Copy(i));
    }
    if !info_done {
        parts.push(Part::New(*b"LIST", rep.info.unwrap().to_vec()));
    }
    if !id3_done {
        parts.push(Part::New(*id3_id, rep.id3.unwrap().to_vec()));
    }

    // Taille totale du conteneur.
    let mut total: u64 = 4; // type de forme
    for p in &parts {
        let size = match p {
            Part::Copy(i) => layout.chunks[*i].size as u64,
            Part::New(_, d) => d.len() as u64,
        };
        total += 8 + size + (size & 1);
    }
    if total > u32::MAX as u64 {
        return Err(invalid("fichier trop grand pour le format WAV/AIFF"));
    }
    body.clear();
    out.write_all(if le { b"RIFF" } else { b"FORM" })?;
    out.write_all(&size_bytes(total as u32, le))?;
    out.write_all(&layout.form_type)?;
    let mut buf = vec![0u8; 1 << 20];
    for p in &parts {
        match p {
            Part::Copy(i) => {
                let c = &layout.chunks[*i];
                out.write_all(&c.id)?;
                out.write_all(&size_bytes(c.size, le))?;
                src.seek(SeekFrom::Start(c.data_offset))?;
                let mut left = c.size as u64;
                while left > 0 {
                    let n = left.min(buf.len() as u64) as usize;
                    src.read_exact(&mut buf[..n])?;
                    out.write_all(&buf[..n])?;
                    left -= n as u64;
                }
                if c.size & 1 == 1 {
                    out.write_all(&[0])?;
                }
            }
            Part::New(id, data) => {
                out.write_all(id)?;
                out.write_all(&size_bytes(data.len() as u32, le))?;
                out.write_all(data)?;
                if data.len() & 1 == 1 {
                    out.write_all(&[0])?;
                }
            }
        }
    }
    Ok(())
}

enum Part {
    Copy(usize),
    New([u8; 4], Vec<u8>),
}

fn size_bytes(v: u32, le: bool) -> [u8; 4] {
    if le {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}

fn find_info_index<R: Read + Seek>(layout: &Layout, r: &mut R) -> io::Result<Option<usize>> {
    for (i, c) in layout.chunks.iter().enumerate() {
        if c.is(b"LIST") && c.size >= 4 {
            r.seek(SeekFrom::Start(c.data_offset))?;
            let mut t = [0u8; 4];
            r.read_exact(&mut t)?;
            if &t == b"INFO" {
                return Ok(Some(i));
            }
        }
    }
    Ok(None)
}

/// Le tag ID3 contenu dans un bloc « id3 », s'il existe.
pub fn read_id3_chunk<R: Read + Seek>(layout: &Layout, r: &mut R) -> io::Result<Option<Vec<u8>>> {
    match layout.chunks.iter().find(|c| c.is_id3()) {
        Some(c) => Ok(Some(read_chunk(r, c)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    pub fn tiny_wav(extra: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut chunks: Vec<u8> = Vec::new();
        let fmt: [u8; 16] = [1, 0, 1, 0, 0x44, 0xac, 0, 0, 0x88, 0x58, 1, 0, 2, 0, 16, 0];
        chunks.extend(b"fmt ");
        chunks.extend(16u32.to_le_bytes());
        chunks.extend(fmt);
        let data: Vec<u8> = (0..1001u32).map(|i| (i % 251) as u8).collect();
        chunks.extend(b"data");
        chunks.extend((data.len() as u32).to_le_bytes());
        chunks.extend(&data);
        chunks.push(0);
        for (id, d) in extra {
            chunks.extend(id);
            chunks.extend((d.len() as u32).to_le_bytes());
            chunks.extend(d);
            if d.len() & 1 == 1 {
                chunks.push(0);
            }
        }
        let mut out = b"RIFF".to_vec();
        out.extend(((chunks.len() + 4) as u32).to_le_bytes());
        out.extend(b"WAVE");
        out.extend(chunks);
        out
    }

    #[test]
    fn info_latin1_lu_et_conserve() {
        let info = build_info(&[
            InfoItem { id: *b"INAM", raw: b"Caf\xe9".to_vec() }, // Windows-1252
            InfoItem { id: *b"ISFT", raw: b"Logiciel X".to_vec() },
        ]);
        let wav = tiny_wav(&[(*b"LIST", info), (*b"bext", vec![7; 33])]);
        let mut cur = Cursor::new(wav.clone());
        let layout = read_layout(&mut cur).unwrap();
        let (_, items) = find_info(&layout, &mut cur).unwrap().unwrap();
        assert_eq!(decode_text(&items[0].raw), "Café");

        // On remplace seulement INAM ; ISFT et le bloc bext doivent rester identiques.
        let mut new_items = items.clone();
        new_items[0].raw = "Déjà".as_bytes().to_vec();
        let new_info = build_info(&new_items);
        let id3 = b"ID3\x03\x00\x00\x00\x00\x00\x00".to_vec();
        let mut out = Vec::new();
        rewrite(&mut cur, &layout, &Replacement { info: Some(&new_info), id3: Some(&id3) }, &mut out).unwrap();

        let mut c2 = Cursor::new(out);
        let l2 = read_layout(&mut c2).unwrap();
        let (_, items2) = find_info(&l2, &mut c2).unwrap().unwrap();
        assert_eq!(decode_text(&items2[0].raw), "Déjà");
        assert_eq!(items2[1].raw, b"Logiciel X");
        let bext = l2.chunks.iter().find(|c| c.is(b"bext")).unwrap();
        assert_eq!(read_chunk(&mut c2, bext).unwrap(), vec![7; 33]);
        // Audio identique
        let d1 = layout.chunks.iter().find(|c| c.is(b"data")).unwrap();
        let d2 = l2.chunks.iter().find(|c| c.is(b"data")).unwrap();
        assert_eq!(read_chunk(&mut cur, d1).unwrap(), read_chunk(&mut c2, d2).unwrap());
        assert_eq!(read_id3_chunk(&l2, &mut c2).unwrap().unwrap(), id3);
        // Taille RIFF cohérente
        let total = c2.get_ref().len() as u32 - 8;
        assert_eq!(u32::from_le_bytes(c2.get_ref()[4..8].try_into().unwrap()), total);
    }
}
