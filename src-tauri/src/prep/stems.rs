//! Séparation des pistes (phase 8) : voix, batterie, basse, autres, avec le modèle libre
//! Demucs v4 « hybrid transformer » (Meta, licence MIT), dans sa version convertie pour
//! ONNX Runtime (moteur de Microsoft, licence MIT).
//!
//! Rien n'est fourni avec l'installateur : à la première utilisation, et avec l'accord de
//! l'utilisateur, l'application télécharge
//! - le moteur ONNX Runtime 1.23.2 (paquet officiel NuGet de Microsoft : seul le fichier de
//!   l'ordinateur est lu dans le paquet, ~5 à 11 Mo) ;
//! - le modèle (~210 Mo).
//! Chaque fichier est vérifié par son empreinte SHA-256 avant d'être gardé.
//!
//! Traitement (identique à la conversion d'origine) : son en 44,1 kHz stéréo, fenêtres de
//! 343 980 échantillons (7,8 s) ; le modèle reçoit le son et son spectre (TFCT de 4096 points,
//! pas de 1024) et rend, pour chaque piste, un son et un spectre qu'on additionne après
//! TFCT inverse. Chaque fenêtre est centrée sur la partie gardée (contexte avant et après).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::audio::decode::Decoder;
use crate::remaster::engine::Wav24;

const RUNTIME_PKG: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.23.2/microsoft.ml.onnxruntime.1.23.2.nupkg";
const RUNTIME_PKG_SIZE: u64 = 123_371_574;
const MODEL_URL: &str = "https://huggingface.co/gentij/htdemucs-ort/resolve/main/htdemucs.ort";
const MODEL_SHA256: &str = "09dc165512d8ef7480bcb2cacea9dda82d571f8dbf421d8c44a2ca5568bec729";
const MODEL_SIZE: u64 = 209_884_896;

/// Fichier du moteur pour cet ordinateur : (chemin dans le paquet, nom local, SHA-256).
fn runtime_entry() -> Option<(&'static str, &'static str, &'static str)> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Some(("runtimes/win-x64/native/onnxruntime.dll", "onnxruntime.dll", "dec964ab1ee36cc9b0ae247d13b376627992fc57dec0454354017ab8fd84f1ea"))
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(("runtimes/osx-arm64/native/libonnxruntime.dylib", "libonnxruntime.dylib", "d306d2bc768540766c7ed8a1e0ff05d2870c77a934ebeee4a7bafa1b732ef299"))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(("runtimes/osx-x64/native/libonnxruntime.dylib", "libonnxruntime.dylib", "8c9c78de65ea3786f987c0d980e9c1b13a3a5fbc6b3e2965ba05b450e6e4c054"))
    } else {
        None
    }
}

pub const SAMPLE_RATE: u32 = 44_100;
const T: usize = 343_980;
const HOP: usize = 171_990;
const NFFT: usize = 4096;
const STFT_HOP: usize = 1024;
const F: usize = 2048;
const FRAMES: usize = 336;
/// Ordre des pistes rendues par le modèle.
pub const STEMS: [&str; 4] = ["drums", "bass", "other", "vocals"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Cet ordinateur peut utiliser la séparation.
    pub supported: bool,
    pub installed: bool,
    /// Taille à télécharger (octets) si rien n'est installé.
    pub download_bytes: u64,
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("stems")
}

fn sha256_file(p: &Path) -> Option<String> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(hex(&h.finalize()))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn status(data_dir: &Path) -> Status {
    let d = dir(data_dir);
    let Some((_, name, _)) = runtime_entry() else {
        return Status { supported: false, installed: false, download_bytes: 0 };
    };
    // Présence et taille suffisent ici ; l'empreinte est vérifiée au téléchargement.
    let rt = d.join(name).exists();
    let model = std::fs::metadata(d.join("htdemucs.ort")).map(|m| m.len() == MODEL_SIZE).unwrap_or(false);
    Status { supported: true, installed: rt && model, download_bytes: if rt && model { 0 } else { MODEL_SIZE + 12_000_000 } }
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent("dj-library-manager (stems)")
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .build()
        .into()
}

fn range(agent: &ureq::Agent, url: &str, start: u64, len: u64) -> Result<Vec<u8>, String> {
    let mut res = agent
        .get(url)
        .header("Range", &format!("bytes={start}-{}", start + len - 1))
        .call()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(len as usize);
    res.body_mut().as_reader().take(len).read_to_end(&mut out).map_err(|e| e.to_string())?;
    if out.len() as u64 != len {
        return Err("réponse incomplète".into());
    }
    Ok(out)
}

fn u16le(b: &[u8], i: usize) -> usize {
    u16::from_le_bytes([b[i], b[i + 1]]) as usize
}
fn u32le(b: &[u8], i: usize) -> u64 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as u64
}

/// Lit un seul fichier dans le paquet zip distant (requêtes partielles).
fn zip_entry(agent: &ureq::Agent, url: &str, total: u64, name: &str) -> Result<Vec<u8>, String> {
    let tail_len = 65_536.min(total);
    let tail = range(agent, url, total - tail_len, tail_len)?;
    let eocd = tail.windows(4).rposition(|w| w == [0x50, 0x4b, 0x05, 0x06]).ok_or("paquet illisible")?;
    let cd_size = u32le(&tail, eocd + 12);
    let cd_off = u32le(&tail, eocd + 16);
    let cd = range(agent, url, cd_off, cd_size)?;
    let mut p = 0;
    while p + 46 <= cd.len() && cd[p..p + 4] == [0x50, 0x4b, 0x01, 0x02] {
        let method = u16le(&cd, p + 10);
        let csize = u32le(&cd, p + 20);
        let usize_ = u32le(&cd, p + 24);
        let (nl, el, kl) = (u16le(&cd, p + 28), u16le(&cd, p + 30), u16le(&cd, p + 32));
        let lho = u32le(&cd, p + 42);
        let n = String::from_utf8_lossy(&cd[p + 46..p + 46 + nl]);
        if n == name {
            let lh = range(agent, url, lho, 30)?;
            let off = lho + 30 + u16le(&lh, 26) as u64 + u16le(&lh, 28) as u64;
            let data = range(agent, url, off, csize)?;
            let raw = match method {
                0 => data,
                8 => {
                    let mut out = Vec::with_capacity(usize_ as usize);
                    flate2::read::DeflateDecoder::new(&data[..]).read_to_end(&mut out).map_err(|e| e.to_string())?;
                    out
                }
                m => return Err(format!("compression {m}")),
            };
            return Ok(raw);
        }
        p += 46 + nl + el + kl;
    }
    Err("fichier absent du paquet".into())
}

/// Télécharge et vérifie le moteur et le modèle. `progress(fait, total)` en octets.
pub fn install(data_dir: &Path, cancelled: impl Fn() -> bool, progress: impl Fn(u64, u64)) -> Result<(), String> {
    let (entry, name, sum) = runtime_entry().ok_or("non pris en charge sur cet ordinateur")?;
    let d = dir(data_dir);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let ag = agent();
    let total = MODEL_SIZE + 12_000_000;
    let rt = d.join(name);
    if sha256_file(&rt).as_deref() != Some(sum) {
        let bytes = zip_entry(&ag, RUNTIME_PKG, RUNTIME_PKG_SIZE, entry)?;
        if hex(&Sha256::digest(&bytes)) != sum {
            return Err("empreinte du moteur incorrecte".into());
        }
        let tmp = d.join(format!("{name}.part"));
        std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &rt).map_err(|e| e.to_string())?;
    }
    progress(12_000_000, total);
    let model = d.join("htdemucs.ort");
    if std::fs::metadata(&model).map(|m| m.len() == MODEL_SIZE).unwrap_or(false) && sha256_file(&model).as_deref() == Some(MODEL_SHA256) {
        progress(total, total);
        return Ok(());
    }
    let tmp = d.join("htdemucs.ort.part");
    let mut res = ag.get(MODEL_URL).call().map_err(|e| e.to_string())?;
    let mut reader = res.body_mut().with_config().limit(MODEL_SIZE + 1024).reader();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut got = 0u64;
    loop {
        if cancelled() {
            drop(out);
            let _ = std::fs::remove_file(&tmp);
            return Err("annulé".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n as u64;
        progress(12_000_000 + got, total);
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if got != MODEL_SIZE || hex(&h.finalize()) != MODEL_SHA256 {
        let _ = std::fs::remove_file(&tmp);
        return Err("empreinte du modèle incorrecte".into());
    }
    std::fs::rename(&tmp, &model).map_err(|e| e.to_string())?;
    Ok(())
}

// =====================================================================
// Modèle
// =====================================================================

static SESSION: OnceLock<Mutex<ort::session::Session>> = OnceLock::new();

fn session(data_dir: &Path) -> Result<&'static Mutex<ort::session::Session>, String> {
    if let Some(s) = SESSION.get() {
        return Ok(s);
    }
    let d = dir(data_dir);
    let (_, name, _) = runtime_entry().ok_or("non pris en charge")?;
    ort::init_from(d.join(name)).map_err(|e| e.to_string())?.with_name("dj-library-manager").commit();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let s = ort::session::Session::builder()
        .map_err(|e| e.to_string())?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        .with_intra_threads(threads)
        .map_err(|e| e.to_string())?
        .commit_from_file(d.join("htdemucs.ort"))
        .map_err(|e| e.to_string())?;
    let _ = SESSION.set(Mutex::new(s));
    SESSION.get().ok_or_else(|| "modèle indisponible".into())
}

fn window() -> Vec<f32> {
    // Fenêtre de Hann symétrique (dénominateur n − 1), comme à la conversion du modèle.
    (0..NFFT).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (NFFT - 1) as f32).cos()).collect()
}

/// TFCT d'un canal (T échantillons) → [F][FRAMES] complexes, bourrage de zéros NFFT/2.
fn stft(x: &[f32], w: &[f32], fft: &dyn rustfft::Fft<f32>) -> Vec<Complex<f32>> {
    let pad = NFFT / 2;
    let mut sig = vec![0f32; T + 2 * pad];
    sig[pad..pad + T].copy_from_slice(x);
    let mut out = vec![Complex::new(0.0, 0.0); F * FRAMES];
    let mut buf = vec![Complex::new(0.0f32, 0.0); NFFT];
    for fr in 0..FRAMES {
        let s = fr * STFT_HOP;
        for i in 0..NFFT {
            buf[i] = Complex::new(sig.get(s + i).copied().unwrap_or(0.0) * w[i], 0.0);
        }
        fft.process(&mut buf);
        for k in 0..F {
            out[k * FRAMES + fr] = buf[k];
        }
    }
    out
}

/// TFCT inverse (spectre [F][FRAMES]) ajoutée à `out` (T échantillons).
fn istft_add(spec_re: &[f32], spec_im: &[f32], w: &[f32], ifft: &dyn rustfft::Fft<f32>, out: &mut [f32]) {
    let pad = NFFT / 2;
    let len = T + 2 * pad;
    let mut acc = vec![0f32; len];
    let mut norm = vec![0f32; len];
    let mut buf = vec![Complex::new(0.0f32, 0.0); NFFT];
    for fr in 0..FRAMES {
        for k in 0..F {
            buf[k] = Complex::new(spec_re[k * FRAMES + fr], spec_im[k * FRAMES + fr]);
        }
        buf[F] = Complex::new(0.0, 0.0);
        for k in 1..F {
            buf[NFFT - k] = buf[k].conj();
        }
        ifft.process(&mut buf);
        let s = fr * STFT_HOP;
        for i in 0..NFFT {
            if s + i < len {
                acc[s + i] += buf[i].re / NFFT as f32 * w[i];
                norm[s + i] += w[i] * w[i];
            }
        }
    }
    for i in 0..T {
        let n = norm[pad + i];
        if n > 1e-10 {
            out[i] += acc[pad + i] / n;
        }
    }
}

/// Une fenêtre de T trames stéréo (L puis R) → 4 pistes × 2 canaux × T.
fn infer(sess: &mut ort::session::Session, left: &[f32], right: &[f32]) -> Result<Vec<f32>, String> {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(NFFT);
    let ifft = planner.plan_fft_inverse(NFFT);
    let w = window();
    let mut time = Vec::with_capacity(2 * T);
    time.extend_from_slice(left);
    time.extend_from_slice(right);
    let (sl, sr) = (stft(left, &w, fft.as_ref()), stft(right, &w, fft.as_ref()));
    let mut spec = Vec::with_capacity(4 * F * FRAMES);
    spec.extend(sl.iter().map(|c| c.re));
    spec.extend(sl.iter().map(|c| c.im));
    spec.extend(sr.iter().map(|c| c.re));
    spec.extend(sr.iter().map(|c| c.im));
    let tv = ort::value::TensorRef::from_array_view(([1usize, 2, T], &time[..])).map_err(|e| e.to_string())?;
    let sv = ort::value::TensorRef::from_array_view(([1usize, 4, F, FRAMES], &spec[..])).map_err(|e| e.to_string())?;
    let mut outs = sess.run(ort::inputs!["input" => tv, "x" => sv]).map_err(|e| e.to_string())?;
    let t_out = outs.remove("add_67").ok_or("sortie temporelle absente")?;
    let f_out = outs.remove("output").ok_or("sortie spectrale absente")?;
    let (_, tdata) = t_out.try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    let (_, fdata) = f_out.try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    if tdata.len() != 4 * 2 * T || fdata.len() != 4 * 4 * F * FRAMES {
        return Err("sorties de taille inattendue".into());
    }
    let mut res = tdata.to_vec();
    let plane = F * FRAMES;
    for s in 0..4 {
        for c in 0..2 {
            let base = (s * 4 + c * 2) * plane;
            let (re, im) = (&fdata[base..base + plane], &fdata[base + plane..base + 2 * plane]);
            istft_add(re, im, &w, ifft.as_ref(), &mut res[(s * 2 + c) * T..(s * 2 + c + 1) * T]);
        }
    }
    Ok(res)
}

/// Son du fichier en 44,1 kHz stéréo (deux canaux séparés).
fn load(path: &Path) -> Result<(Vec<f32>, Vec<f32>), String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as usize;
    let rate = dec.sample_rate;
    let (mut l, mut r) = (Vec::new(), Vec::new());
    let mut buf = Vec::new();
    while dec.next_block(&mut buf)? {
        for f in buf.chunks(ch) {
            l.push(f[0]);
            r.push(if ch >= 2 { f[1] } else { f[0] });
        }
    }
    if rate != SAMPLE_RATE {
        // Rééchantillonnage linéaire vers 44,1 kHz.
        let step = rate as f64 / SAMPLE_RATE as f64;
        let n = (l.len() as f64 / step) as usize;
        let rs = |v: &[f32]| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let p = i as f64 * step;
                    let k = p as usize;
                    let f = (p - k as f64) as f32;
                    let a = v.get(k).copied().unwrap_or(0.0);
                    let b = v.get(k + 1).copied().unwrap_or(a);
                    a + (b - a) * f
                })
                .collect()
        };
        return Ok((rs(&l), rs(&r)));
    }
    Ok((l, r))
}

/// Fenêtre de T trames commençant à `start` (peut être négatif ou dépasser : zéros).
fn slice(v: &[f32], start: isize) -> Vec<f32> {
    (0..T as isize).map(|i| { let k = start + i; if k >= 0 { v.get(k as usize).copied().unwrap_or(0.0) } else { 0.0 } }).collect()
}

/// Sépare le morceau `src` ; écrit chaque piste demandée (`want`, indices de [`STEMS`], ou
/// 4 = instrumental : batterie + basse + autres) dans un WAV 24 bits 44,1 kHz temporaire.
pub fn separate(data_dir: &Path, src: &Path, want: &[usize], outputs: &[PathBuf], cancelled: impl Fn() -> bool, progress: impl Fn(u64, u64)) -> Result<(), String> {
    let sess = session(data_dir)?;
    let (l, r) = load(src)?;
    let n = l.len();
    let mut writers: Vec<Wav24> = outputs.iter().map(|p| Wav24::create(p, SAMPLE_RATE, 2)).collect::<Result<_, _>>().map_err(|e| e.to_string())?;
    let chunks = n.div_ceil(HOP).max(1);
    let mut s = 0usize;
    let mut done = 0u64;
    while s < n {
        if cancelled() {
            return Err("annulé".into());
        }
        // Fenêtre centrée sur la partie gardée [s, s + HOP).
        let w0 = s as isize - (HOP / 2) as isize;
        let out = {
            let mut g = sess.lock().map_err(|_| "modèle occupé".to_string())?;
            infer(&mut g, &slice(&l, w0), &slice(&r, w0))?
        };
        let keep = HOP.min(n - s);
        let off = HOP / 2;
        for (wi, &stem) in want.iter().enumerate() {
            let mut inter = Vec::with_capacity(keep * 2);
            for i in 0..keep {
                let get = |st: usize, c: usize| out[(st * 2 + c) * T + off + i];
                let (a, b) = if stem == 4 { (get(0, 0) + get(1, 0) + get(2, 0), get(0, 1) + get(1, 1) + get(2, 1)) } else { (get(stem, 0), get(stem, 1)) };
                inter.push(a);
                inter.push(b);
            }
            writers[wi].write(&inter).map_err(|e| e.to_string())?;
        }
        s += keep;
        done += 1;
        progress(done, chunks as u64);
    }
    for w in writers {
        w.finish().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Part de voix (0 à 1) : énergie de la piste « voix » / énergie totale, sur trois extraits
/// de 7,8 s (à 30, 50 et 70 % du morceau).
pub fn vocal_ratio(data_dir: &Path, src: &Path) -> Result<f64, String> {
    let sess = session(data_dir)?;
    let (l, r) = load(src)?;
    let n = l.len();
    let (mut voc, mut all) = (0f64, 0f64);
    for at in [0.3, 0.5, 0.7] {
        let start = ((n as f64 * at) as isize - (T / 2) as isize).max(0);
        let out = {
            let mut g = sess.lock().map_err(|_| "modèle occupé".to_string())?;
            infer(&mut g, &slice(&l, start), &slice(&r, start))?
        };
        for st in 0..4 {
            let e: f64 = out[st * 2 * T..(st + 1) * 2 * T].iter().map(|x| (*x as f64).powi(2)).sum();
            all += e;
            if st == 3 {
                voc += e;
            }
        }
    }
    Ok(if all > 0.0 { voc / all } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tfct_aller_retour() {
        // TFCT puis TFCT inverse : le son revient (hors bord, aux erreurs d'arrondi près).
        let x: Vec<f32> = (0..T).map(|i| (i as f32 * 0.013).sin() * 0.5 + (i as f32 * 0.0021).cos() * 0.2).collect();
        let mut planner = FftPlanner::<f32>::new();
        let (fft, ifft) = (planner.plan_fft_forward(NFFT), planner.plan_fft_inverse(NFFT));
        let w = window();
        let s = stft(&x, &w, fft.as_ref());
        let re: Vec<f32> = s.iter().map(|c| c.re).collect();
        let im: Vec<f32> = s.iter().map(|c| c.im).collect();
        let mut y = vec![0f32; T];
        istft_add(&re, &im, &w, ifft.as_ref(), &mut y);
        let err = (5000..T - 5000).map(|i| (x[i] - y[i]).abs()).fold(0.0, f32::max);
        assert!(err < 1e-3, "{err}");
    }

    /// Téléchargement réel (pas lancé d'habitude) : DJLM_STEMS_DIR.
    #[test]
    #[ignore]
    fn installation_reelle() {
        let Ok(d) = std::env::var("DJLM_STEMS_DIR") else { return };
        let t0 = std::time::Instant::now();
        install(Path::new(&d), || false, |a, b| if a % (50 << 20) < (1 << 20) { println!("STEMS {} / {} Mo", a >> 20, b >> 20) }).unwrap();
        println!("STEMS installé en {:.0} s : {:?}", t0.elapsed().as_secs_f64(), status(Path::new(&d)));
    }

    /// Essai réel (pas lancé d'habitude) : DJLM_STEMS_DIR (moteur + modèle), DJLM_DIAG_FILE.
    #[test]
    #[ignore]
    fn separation_reelle() {
        let (Ok(d), Ok(f)) = (std::env::var("DJLM_STEMS_DIR"), std::env::var("DJLM_DIAG_FILE")) else { return };
        let data = PathBuf::from(d);
        let out = tempfile::tempdir().unwrap();
        let t0 = std::time::Instant::now();
        let ratio = vocal_ratio(&data, Path::new(&f)).unwrap();
        println!("STEMS part de voix {:.2} ({:.1} s)", ratio, t0.elapsed().as_secs_f64());
        let t0 = std::time::Instant::now();
        let paths = vec![out.path().join("inst.wav"), out.path().join("voix.wav")];
        separate(&data, Path::new(&f), &[4, 3], &paths, || false, |a, b| if a % 10 == 0 { println!("STEMS {a}/{b}") }).unwrap();
        println!("STEMS séparation {:.1} s", t0.elapsed().as_secs_f64());
        // Contrôle : instrumental + voix redonnent le morceau (écart relatif en dB).
        let (ol, _) = load(Path::new(&f)).unwrap();
        let read = |p: &Path| { let (l, _) = load(p).unwrap(); l };
        let (inst, voc) = (read(&paths[0]), read(&paths[1]));
        let n = ol.len().min(inst.len()).min(voc.len());
        let e = |v: &mut dyn Iterator<Item = f64>| v.map(|x| x * x).sum::<f64>();
        let orig = e(&mut ol[..n].iter().map(|x| *x as f64));
        let resid = e(&mut (0..n).map(|i| (ol[i] - inst[i] - voc[i]) as f64));
        let ev = e(&mut voc[..n].iter().map(|x| *x as f64));
        println!("STEMS écart de reconstruction {:.1} dB, voix {:.1} dB sous le mélange", 10.0 * (resid / orig).log10(), 10.0 * (ev / orig).log10());
        if let Ok(keep) = std::env::var("DJLM_STEMS_KEEP") {
            for p in &paths {
                std::fs::copy(p, Path::new(&keep).join(p.file_name().unwrap())).unwrap();
            }
        }
    }
}
