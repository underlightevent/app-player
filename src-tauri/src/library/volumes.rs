//! Volumes (disques) : identifiant stable, nom et point de montage actuel.
//!
//! Les chemins des morceaux sont enregistrés par rapport à leur volume. Si la
//! lettre de lecteur change (E: devient F:) ou si le point de montage change sur
//! macOS, l'identifiant du volume permet de retrouver les fichiers.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedVolume {
    /// Identifiant stable (numéro de série sous Windows, UUID sous macOS).
    pub uid: String,
    pub label: String,
    /// Racine du volume : « E:\ » ou « /Volumes/MUSIQUE ».
    pub mount: PathBuf,
}

/// Le volume qui contient `path`, et le chemin relatif à sa racine (séparateurs « / »).
pub fn locate(path: &Path) -> std::io::Result<(MountedVolume, String)> {
    let canonical = dunce_canonical(path)?;
    let vol = volume_of(&canonical)?;
    let rel = canonical.strip_prefix(&vol.mount).unwrap_or(&canonical);
    Ok((vol, to_rel_string(rel)))
}

pub fn to_rel_string(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn join_rel(mount: &Path, rel: &str) -> PathBuf {
    let mut p = mount.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

/// Chemin canonique sans le préfixe « \\?\ » de Windows.
fn dunce_canonical(path: &Path) -> std::io::Result<PathBuf> {
    let c = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = c.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            if !rest.starts_with("UNC\\") {
                return Ok(PathBuf::from(rest));
            }
        }
    }
    Ok(c)
}

// ---------------- Windows ----------------
#[cfg(windows)]
mod imp {
    use super::MountedVolume;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNameW,
    };

    const DRIVE_NO_ROOT_DIR: u32 = 1;
    const DRIVE_CDROM: u32 = 5;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn info(root: &str) -> Option<MountedVolume> {
        let w = wide(root);
        let mut label = [0u16; 261];
        let mut serial: u32 = 0;
        // SAFETY : tampons de taille correcte, pointeurs nuls permis pour les champs ignorés.
        let ok = unsafe {
            GetVolumeInformationW(
                w.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                &mut serial,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            return None;
        }
        let len = label.iter().position(|c| *c == 0).unwrap_or(0);
        let label = String::from_utf16_lossy(&label[..len]);
        let letter = root.trim_end_matches('\\').to_string();
        Some(MountedVolume {
            uid: format!("win:{serial:08X}"),
            label: if label.is_empty() { letter } else { label },
            mount: PathBuf::from(root),
        })
    }

    pub fn volume_of(path: &Path) -> std::io::Result<MountedVolume> {
        let w: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut buf = [0u16; 1024];
        // SAFETY : tampon de 1024 caractères, taille transmise.
        let ok = unsafe { GetVolumePathNameW(w.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let len = buf.iter().position(|c| *c == 0).unwrap_or(0);
        let root = String::from_utf16_lossy(&buf[..len]);
        info(&root).ok_or_else(|| std::io::Error::other(format!("volume illisible : {root}")))
    }

    pub fn mounted() -> Vec<MountedVolume> {
        // SAFETY : appel sans paramètre.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 2..26u32 {
            // A: et B: ignorés (lecteurs de disquettes : accès lent)
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = format!("{}:\\", (b'A' + i as u8) as char);
            let w = wide(&root);
            // SAFETY : chaîne terminée par zéro.
            let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
            if kind == DRIVE_NO_ROOT_DIR || kind == DRIVE_CDROM {
                continue;
            }
            if let Some(v) = info(&root) {
                out.push(v);
            }
        }
        out
    }
}

// ---------------- macOS ----------------
#[cfg(target_os = "macos")]
mod imp {
    use super::MountedVolume;
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    fn mount_point(path: &Path) -> std::io::Result<PathBuf> {
        let c = CString::new(path.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY : chemin terminé par zéro, structure initialisée.
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let name = unsafe { CStr::from_ptr(st.f_mntonname.as_ptr()) };
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
    }

    fn plist_value(plist: &str, key: &str) -> Option<String> {
        let k = format!("<key>{key}</key>");
        let start = plist.find(&k)? + k.len();
        let rest = &plist[start..];
        let s = rest.find("<string>")? + "<string>".len();
        let e = rest[s..].find("</string>")?;
        Some(rest[s..s + e].to_string())
    }

    fn info(mount: &Path) -> Option<MountedVolume> {
        let out = std::process::Command::new("/usr/sbin/diskutil").arg("info").arg("-plist").arg(mount).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let uuid = plist_value(&text, "VolumeUUID").or_else(|| plist_value(&text, "DiskUUID"));
        let name = plist_value(&text, "VolumeName").unwrap_or_else(|| {
            mount.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Macintosh HD".into())
        });
        let uid = match uuid {
            Some(u) => format!("mac:{u}"),
            None => format!("mac:name:{name}"),
        };
        Some(MountedVolume { uid, label: name, mount: mount.to_path_buf() })
    }

    pub fn volume_of(path: &Path) -> std::io::Result<MountedVolume> {
        let mp = mount_point(path)?;
        info(&mp).ok_or_else(|| std::io::Error::other("volume illisible"))
    }

    /// Tous les volumes montés, d'après la liste du système (getmntinfo).
    /// Sur les Mac récents, le disque interne est en deux parties : « / » (système, en lecture
    /// seule) et « /System/Volumes/Data » (les fichiers de l'utilisateur). Les deux sont gardés,
    /// ainsi que les disques externes de /Volumes ; les volumes techniques sont ignorés.
    pub fn mounted() -> Vec<MountedVolume> {
        let mut points: Vec<PathBuf> = Vec::new();
        let mut buf: *mut libc::statfs = std::ptr::null_mut();
        // SAFETY : getmntinfo remplit `buf` avec un tableau géré par le système, valable jusqu'au prochain appel.
        let n = unsafe { libc::getmntinfo(&mut buf, libc::MNT_NOWAIT) };
        if n > 0 && !buf.is_null() {
            // SAFETY : `n` entrées valides à partir de `buf`.
            let list = unsafe { std::slice::from_raw_parts(buf, n as usize) };
            for st in list {
                let name = unsafe { CStr::from_ptr(st.f_mntonname.as_ptr()) };
                let fstype = unsafe { CStr::from_ptr(st.f_fstypename.as_ptr()) }.to_string_lossy();
                if matches!(fstype.as_ref(), "devfs" | "autofs" | "nullfs" | "lifs") {
                    continue;
                }
                let p = PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes()));
                let keep = p == Path::new("/") || p == Path::new("/System/Volumes/Data") || p.starts_with("/Volumes");
                if keep && !points.contains(&p) {
                    points.push(p);
                }
            }
        }
        if points.is_empty() {
            points.push(PathBuf::from("/"));
        }
        points.iter().filter_map(|p| info(p)).collect()
    }
}

// ---------------- Autres systèmes (tests sous Linux) ----------------
#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::MountedVolume;
    use std::path::{Path, PathBuf};

    pub fn volume_of(_path: &Path) -> std::io::Result<MountedVolume> {
        Ok(MountedVolume { uid: "root".into(), label: "/".into(), mount: PathBuf::from("/") })
    }
    pub fn mounted() -> Vec<MountedVolume> {
        vec![MountedVolume { uid: "root".into(), label: "/".into(), mount: PathBuf::from("/") }]
    }
}

pub use imp::{mounted, volume_of};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossier_temporaire_localise() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a").join("b")).unwrap();
        let (vol, rel) = locate(&dir.path().join("a").join("b")).unwrap();
        assert!(!vol.uid.is_empty());
        assert!(rel.ends_with("a/b"), "{rel}");
        let back = join_rel(&vol.mount, &rel);
        assert!(back.exists());
        assert!(mounted().iter().any(|m| m.uid == vol.uid));
    }
}
