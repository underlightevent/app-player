//! Contrôle d'une clé USB avant une soirée (phase 8). Lecture seule : rien n'est modifié.
//!
//! Pour chaque fichier audio du dossier choisi : lisible ? format, fréquence et profondeur
//! acceptés par le matériel choisi ? chemin trop long ? pochette très lourde ? Et pour la clé :
//! système de fichiers reconnu par le matériel ?
//!
//! Profils (manuels Pioneer DJ / AlphaTheta, prudents) :
//! - « old » : anciens lecteurs (CDJ-2000, CDJ-900, CDJ-850, XDJ-700…) : MP3, AAC, WAV, AIFF ;
//!   44,1 ou 48 kHz ; 16 ou 24 bits ; clé en FAT32 ou HFS+ ;
//! - « nxs2 » : CDJ-2000NXS2, XDJ-1000MK2, XDJ-RX2… : en plus FLAC et ALAC, jusqu'à 96 kHz ;
//!   FAT32 ou HFS+ ;
//! - « cdj3000 » : CDJ-3000, XDJ-RX3, XDJ-XZ… : comme « nxs2 », exFAT accepté.
//! Partout : WAV en virgule flottante ou 32 bits refusés, NTFS non lu.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub path: String,
    /// unreadable, format, rate, depth, float, path_long, big_file, cover_heavy
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Système de fichiers de la clé (« FAT32 », « exFAT », « NTFS », « HFS+ »…), si connu.
    pub filesystem: Option<String>,
    /// Le système de fichiers n'est pas lu par le matériel choisi.
    pub filesystem_ok: Option<bool>,
    pub files: usize,
    pub ok: usize,
    pub issues: Vec<Issue>,
    /// Dossier « PIONEER » (export Rekordbox) présent.
    pub rekordbox_export: bool,
    pub bytes: u64,
}

/// Système de fichiers du volume qui contient `path`.
#[cfg(windows)]
pub fn filesystem(path: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    let root = path.components().next()?;
    let mut root: Vec<u16> = Path::new(root.as_os_str()).join("\\").as_os_str().encode_wide().collect();
    root.push(0);
    let mut name = [0u16; 64];
    let ok = unsafe { GetVolumeInformationW(root.as_ptr(), std::ptr::null_mut(), 0, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), name.as_mut_ptr(), name.len() as u32) };
    if ok == 0 {
        return None;
    }
    let n = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..n]))
}

#[cfg(target_os = "macos")]
pub fn filesystem(path: &Path) -> Option<String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let raw: Vec<u8> = st.f_fstypename.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    Some(match String::from_utf8_lossy(&raw).as_ref() {
        "msdos" => "FAT32".to_string(),
        "exfat" => "exFAT".to_string(),
        "hfs" => "HFS+".to_string(),
        "ntfs" => "NTFS".to_string(),
        other => other.to_string(),
    })
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn filesystem(_path: &Path) -> Option<String> {
    None
}

fn fs_ok(fs: &str, profile: &str) -> bool {
    let f = fs.to_uppercase();
    match f.as_str() {
        "FAT32" | "FAT" | "FAT16" | "HFS+" | "HFS" => true,
        "EXFAT" => profile == "cdj3000",
        _ => false,
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if crate::tags::is_audio_path(&p) {
            out.push(p);
        }
    }
}

/// WAV en virgule flottante (codage 3, ou sous-format flottant).
fn wav_float(path: &Path) -> bool {
    use std::io::Read;
    let mut b = [0u8; 64];
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    if f.read(&mut b).unwrap_or(0) < 36 || &b[0..4] != b"RIFF" {
        return false;
    }
    // Recherche du bloc fmt dans les premiers octets (cas courant).
    let pos = b.windows(4).position(|w| w == b"fmt ");
    let Some(p) = pos else { return false };
    let tag = u16::from_le_bytes([b[p + 8], b[p + 9]]);
    tag == 3 || (tag == 0xFFFE && b.len() > p + 33 && u16::from_le_bytes([b[p + 32], b[p + 33]]) == 3)
}

/// Contrôle le dossier `dir` pour le profil de matériel `profile`.
pub fn check(dir: &Path, profile: &str, cancelled: impl Fn() -> bool, progress: impl Fn(u64, u64)) -> Report {
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut r = Report { files: files.len(), rekordbox_export: dir.join("PIONEER").is_dir(), ..Default::default() };
    r.filesystem = filesystem(dir);
    r.filesystem_ok = r.filesystem.as_deref().map(|f| fs_ok(f, profile));
    let fat = r.filesystem.as_deref().map(|f| f.to_uppercase().starts_with("FAT")).unwrap_or(false);
    let max_rate = if profile == "old" { 48_000 } else { 96_000 };
    for (i, p) in files.iter().enumerate() {
        if cancelled() {
            break;
        }
        progress(i as u64 + 1, files.len() as u64);
        let rel = p.strip_prefix(dir).unwrap_or(p).to_string_lossy().replace('\\', "/");
        let mut add = |kind: &str, detail: String| r.issues.push(Issue { path: rel.clone(), kind: kind.into(), detail });
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let mut problems = 0;
        // Lisible : ouverture et premier bloc décodé.
        let readable = crate::audio::decode::Decoder::open(p).and_then(|mut d| {
            let mut b = Vec::new();
            d.next_block(&mut b).map(|ok| ok && !b.is_empty())
        });
        if !matches!(readable, Ok(true)) {
            add("unreadable", readable.err().unwrap_or_default());
            problems += 1;
        }
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let info = crate::tags::read::read_file(p, &[]).ok();
        let codec = info.as_ref().and_then(|i| i.codec.clone()).unwrap_or_default().to_lowercase();
        let lossless_compressed = ext == "flac" || codec.contains("alac");
        if profile == "old" && lossless_compressed {
            add("format", if ext == "flac" { "FLAC".into() } else { "ALAC".into() });
            problems += 1;
        }
        if !["mp3", "m4a", "mp4", "aac", "wav", "aif", "aiff", "flac"].contains(&ext.as_str()) {
            add("format", ext.to_uppercase());
            problems += 1;
        }
        if let Some(i) = &info {
            if let Some(sr) = i.sample_rate {
                if sr > max_rate {
                    add("rate", format!("{} kHz", sr as f64 / 1000.0));
                    problems += 1;
                }
            }
            if let Some(b) = i.bit_depth {
                if b > 24 && !(ext == "wav" && wav_float(p)) {
                    add("depth", format!("{b} bits"));
                    problems += 1;
                }
            }
            if i.cover.width.map(|w| w > 1500).unwrap_or(false) {
                add("cover_heavy", format!("{} px", i.cover.width.unwrap_or(0)));
                problems += 1;
            }
        }
        if ext == "wav" && wav_float(p) {
            add("float", "32 bits flottant".into());
            problems += 1;
        }
        if rel.chars().count() > 240 {
            add("path_long", format!("{} caractères", rel.chars().count()));
            problems += 1;
        }
        if fat && size >= 4 * 1024 * 1024 * 1024 {
            add("big_file", format!("{:.1} Go", size as f64 / 1e9));
            problems += 1;
        }
        r.bytes += size;
        if problems == 0 {
            r.ok += 1;
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signale_flac_pour_ancien_cdj_et_wav_flottant() {
        let dir = tempfile::tempdir().unwrap();
        let usb = dir.path();
        crate::testdata::write(usb, "ok.wav", &crate::testdata::wav_bytes(1));
        crate::testdata::write(usb, "a.flac", &crate::testdata::flac_from_samples(&crate::testdata::samples(20_000, 2)));
        let mut f = crate::testdata::wav_bytes(1);
        f[20] = 3; // codage flottant
        f[34] = 32;
        crate::testdata::write(usb, "float.wav", &f);
        std::fs::write(usb.join("abime.mp3"), b"pas du son").unwrap();
        let r = check(usb, "old", || false, |_, _| {});
        assert_eq!(r.files, 4);
        let kinds = |name: &str| r.issues.iter().filter(|i| i.path == name).map(|i| i.kind.clone()).collect::<Vec<_>>();
        assert!(kinds("ok.wav").is_empty(), "{:?}", r.issues);
        assert_eq!(kinds("a.flac"), ["format"]);
        assert!(kinds("float.wav").contains(&"float".to_string()), "{:?}", r.issues);
        assert!(kinds("abime.mp3").contains(&"unreadable".to_string()));
        assert_eq!(r.ok, 1);
        // Le même FLAC passe sur un CDJ récent.
        let r2 = check(usb, "nxs2", || false, |_, _| {});
        assert!(r2.issues.iter().all(|i| i.path != "a.flac"));
    }
}
