//! Remaster d'un fichier : analyse pour le mode automatique, préréglages, atteinte du volume
//! cible, spectre avant / après, export WAV 24 bits dans un nouveau fichier (l'original n'est
//! jamais modifié), tags et pochette recopiés, ajout à la bibliothèque, annulation.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use ebur128::{EbuR128, Mode};
use rusqlite::{params, Connection, OptionalExtension};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;

use super::dsp::{default_bands, Fade, Processor, Settings, Trim};
use crate::audio::decode::Decoder;
use crate::error::AppError;
use crate::library::Library;
use crate::tasks::TaskHandle;

pub const PRESETS: &[&str] = &["auto", "club", "warm", "bright", "punch", "soft"];

// =====================================================================
// Analyse et mode automatique
// =====================================================================

/// Mesures d'un morceau pour le mode automatique.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measures {
    pub lufs: f64,
    pub peak_db: f64,
    pub rms_db: f64,
    /// Énergie par octave (dB) de chaque zone, relative aux médiums :
    /// infra-graves, graves, médiums, hauts-médiums, aigus.
    pub balance: [f64; 5],
}

const ZONES: [(f64, f64); 5] = [(30.0, 60.0), (60.0, 250.0), (250.0, 2000.0), (2000.0, 6000.0), (6000.0, 16000.0)];
/// Équilibre de référence (dB par octave, relatif aux médiums) : médiane mesurée sur 82
/// morceaux de la bibliothèque de l'utilisateur (volume médian −8,3 LUFS), le 28/11/2026.
const TARGET: [f64; 5] = [4.7, 3.5, 0.0, -4.3, -7.8];

pub fn measure(path: &Path) -> Result<Measures, String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as usize;
    let rate = dec.sample_rate as f64;
    let mut meter = EbuR128::new(ch as u32, dec.sample_rate, Mode::I | Mode::SAMPLE_PEAK).map_err(|e| e.to_string())?;
    let win = 4096;
    let fft = FftPlanner::<f32>::new().plan_fft_forward(win);
    let hann: Vec<f32> = (0..win).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / win as f32).cos()).collect();
    let mut mono: Vec<f32> = Vec::with_capacity(win);
    let mut power = [0f64; 5];
    let (mut sum_sq, mut n) = (0f64, 0u64);
    let mut buf = Vec::new();
    let mut skip = 0usize;
    let mut cbuf = vec![Complex::new(0.0, 0.0); win];
    while dec.next_block(&mut buf)? {
        meter.add_frames_f32(&buf).map_err(|e| e.to_string())?;
        for f in buf.chunks(ch) {
            let m = f.iter().sum::<f32>() / ch as f32;
            sum_sq += (m as f64).powi(2);
            n += 1;
            // Un spectre toutes les ~0,5 s suffit pour l'équilibre.
            if skip > 0 {
                skip -= 1;
                continue;
            }
            mono.push(m);
            if mono.len() == win {
                for i in 0..win {
                    cbuf[i] = Complex::new(mono[i] * hann[i], 0.0);
                }
                fft.process(&mut cbuf);
                for (k, c) in cbuf[..win / 2].iter().enumerate() {
                    let f = k as f64 * rate / win as f64;
                    if let Some(z) = ZONES.iter().position(|(a, b)| f >= *a && f < *b) {
                        power[z] += c.norm_sqr() as f64;
                    }
                }
                mono.clear();
                skip = (rate * 0.5) as usize;
            }
        }
    }
    let lufs = meter.loudness_global().map_err(|e| e.to_string())?;
    if !lufs.is_finite() {
        return Err("son trop court ou silencieux".into());
    }
    let mut peak: f64 = 0.0;
    for c in 0..ch as u32 {
        peak = peak.max(meter.sample_peak(c).map_err(|e| e.to_string())?);
    }
    let dens: Vec<f64> = ZONES.iter().zip(power).map(|((a, b), p)| 10.0 * (p.max(1e-12) / (b / a).log2()).log10()).collect();
    let mut balance = [0f64; 5];
    for i in 0..5 {
        balance[i] = dens[i] - dens[2];
    }
    let rms = (sum_sq / n.max(1) as f64).sqrt();
    Ok(Measures { lufs, peak_db: 20.0 * peak.max(1e-9).log10(), rms_db: 20.0 * rms.max(1e-9).log10(), balance })
}

/// Réglages automatiques doux et bornés, puis variante du préréglage choisi.
pub fn auto_settings(m: &Measures, preset: &str) -> Settings {
    let mut s = Settings { mono_bass_hz: 120.0, comp_on: true, target_lufs: Some(-9.0), ..Default::default() };
    s.bands = default_bands();
    // Correction prudente : un tiers de l'écart, ±3 dB au plus ; les graves ne sont jamais
    // retirés de plus de 1,5 dB (un morceau riche en basses est souvent voulu ainsi en club).
    let corr = |i: usize| ((TARGET[i] - m.balance[i]) / 3.0).clamp(-3.0, 3.0);
    s.bands[0].gain_db = ((corr(0) + corr(1)) / 2.0).clamp(-1.5, 3.0);
    s.bands[3].gain_db = corr(3);
    s.bands[4].gain_db = corr(4);
    // Dynamique : morceau très dynamique (crête bien au-dessus du niveau moyen) = compression
    // un peu plus marquée ; sinon légère.
    let crest = m.peak_db - m.rms_db;
    if crest > 12.0 {
        s.ratio = 2.0;
        s.threshold_db = m.rms_db + 4.0;
    } else {
        s.ratio = 1.5;
        s.threshold_db = m.rms_db + 6.0;
    }
    s.attack_ms = 15.0;
    s.release_ms = 150.0;
    match preset {
        "club" => {
            s.target_lufs = Some(-7.0);
            s.ratio += 0.5;
        }
        "warm" => {
            s.bands[0].gain_db = (s.bands[0].gain_db + 1.5).clamp(-6.0, 6.0);
            s.bands[4].gain_db = (s.bands[4].gain_db - 1.5).clamp(-6.0, 6.0);
        }
        "bright" => {
            s.bands[3].gain_db = (s.bands[3].gain_db + 1.0).clamp(-6.0, 6.0);
            s.bands[4].gain_db = (s.bands[4].gain_db + 2.0).clamp(-6.0, 6.0);
        }
        "punch" => {
            s.ratio = 3.0;
            s.attack_ms = 25.0;
            s.release_ms = 120.0;
            s.threshold_db = m.rms_db + 3.0;
            s.target_lufs = Some(-8.0);
        }
        "soft" => {
            s.comp_on = false;
            s.target_lufs = Some(-12.0);
            s.bands[4].gain_db = (s.bands[4].gain_db - 1.0).clamp(-6.0, 6.0);
        }
        _ => {}
    }
    s.clamped()
}

// =====================================================================
// Traitement d'un fichier
// =====================================================================

/// Passe complète : décode, traite et donne chaque bloc traité à `sink` ; renvoie le volume
/// (LUFS) et la crête (dBFS) du résultat.
fn pass(path: &Path, s: &Settings, gain_db: f64, mut sink: impl FnMut(&[f32]) -> Result<(), String>, task: Option<&TaskHandle>) -> Result<(f64, f64), String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels;
    if ch > 2 {
        return Err("seuls les fichiers mono et stéréo sont pris en charge".into());
    }
    let mut p = Processor::new(s, dec.sample_rate, ch, gain_db);
    let mut meter = EbuR128::new(ch as u32, dec.sample_rate, Mode::I | Mode::SAMPLE_PEAK).map_err(|e| e.to_string())?;
    let mut trim = Trim::new(s, dec.sample_rate);
    // Fondu de sortie sans fin précisée : il faut la longueur exacte du morceau.
    let total = if s.fade_out_s > 0.0 && trim.end.is_none() { Some(count_frames(path)?) } else { None };
    let mut fade = Fade::new(s, dec.sample_rate, trim.kept_len(total));
    let (mut buf, mut cut, mut out) = (Vec::new(), Vec::new(), Vec::new());
    let mut emit = |out: &mut Vec<f32>, fade: &mut Fade, sink: &mut dyn FnMut(&[f32]) -> Result<(), String>| -> Result<(), String> {
        if !out.is_empty() {
            fade.apply(out, ch as usize);
            meter.add_frames_f32(out).map_err(|e| e.to_string())?;
            sink(out)?;
        }
        Ok(())
    };
    while dec.next_block(&mut buf)? {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err("annulé".into());
        }
        let done = trim.cut(&buf, ch as usize, &mut cut);
        p.process(&cut, &mut out);
        emit(&mut out, &mut fade, &mut sink)?;
        if done {
            break;
        }
    }
    p.flush(&mut out);
    emit(&mut out, &mut fade, &mut sink)?;
    let lufs = meter.loudness_global().map_err(|e| e.to_string())?;
    let mut peak: f64 = 0.0;
    for c in 0..ch as u32 {
        peak = peak.max(meter.sample_peak(c).map_err(|e| e.to_string())?);
    }
    Ok((lufs, 20.0 * peak.max(1e-9).log10()))
}

/// Nombre exact de trames du fichier (décodage complet).
fn count_frames(path: &Path) -> Result<u64, String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as u64;
    let mut buf = Vec::new();
    let mut n = 0u64;
    while dec.next_block(&mut buf)? {
        n += buf.len() as u64 / ch;
    }
    Ok(n)
}

/// Gain (dB) à ajouter avant le limiteur pour atteindre le volume cible, et volumes
/// avant / après. Deux passes de mesure (sans puis avec limiteur).
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub before_lufs: f64,
    pub after_lufs: f64,
    pub gain_db: f64,
    pub after_peak_db: f64,
}

/// Poussée maximale dans le limiteur (dB au-delà du gain qui suffirait sans limiteur).
/// Au-delà, le son est écrasé pour un gain de volume minime : on s'arrête là et le volume
/// réellement obtenu est affiché (cas des morceaux déjà très forts).
pub const MAX_PUSH_DB: f64 = 6.0;

pub fn prepare(path: &Path, s: &Settings) -> Result<Prepared, String> {
    let before = crate::djtools::loudness::measure(path)?.lufs;
    let Some(target) = s.target_lufs else {
        let (l, p) = pass(path, s, 0.0, |_| Ok(()), None)?;
        return Ok(Prepared { before_lufs: before, after_lufs: l, gain_db: 0.0, after_peak_db: p });
    };
    // Volume sans limiteur, puis recherche du gain avec le limiteur (méthode de la sécante) :
    // plus le limiteur travaille, moins un dB de gain ajoute de volume.
    let no_lim = Settings { limiter_on: false, ..s.clone() };
    let (l0, _) = pass(path, &no_lim, 0.0, |_| Ok(()), None)?;
    let mut g_prev = target - l0;
    let max_g = (g_prev + MAX_PUSH_DB).min(30.0);
    let (mut l_prev, mut peak) = pass(path, s, g_prev, |_| Ok(()), None)?;
    if !s.limiter_on || (l_prev - target).abs() <= 0.3 {
        return Ok(Prepared { before_lufs: before, after_lufs: l_prev, gain_db: g_prev, after_peak_db: peak });
    }
    // Meilleur essai mesuré (gain, volume, crête).
    let mut best = (g_prev, l_prev, peak);
    let mut g = (g_prev + (target - l_prev) * 2.0).clamp(-30.0, max_g);
    for _ in 0..5 {
        let (l, p) = pass(path, s, g, |_| Ok(()), None)?;
        if (l - target).abs() < (best.1 - target).abs() {
            best = (g, l, p);
        }
        if (l - target).abs() <= 0.3 {
            break;
        }
        let slope = if (l - l_prev).abs() > 0.05 { (g - g_prev) / (l - l_prev) } else { 2.0 };
        let next = (g + (target - l) * slope.clamp(1.0, 12.0)).clamp(-30.0, max_g);
        if (next - g).abs() < 0.01 {
            break; // plafond de poussée atteint
        }
        g_prev = g;
        l_prev = l;
        g = next;
    }
    peak = best.2;
    Ok(Prepared { before_lufs: before, after_lufs: best.1, gain_db: best.0, after_peak_db: peak })
}

/// Nombre de bandes du spectre (de 30 Hz à 18 kHz, espacées en octaves).
pub const SPEC_BANDS: usize = 48;
const SPEC_WIN: usize = 16384;
/// Durée de l'extrait comparé (au milieu de la partie gardée du morceau).
const SPEC_SECS: f64 = 45.0;

/// Mesures d'une version (originale ou traitée) sur l'extrait.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// Niveau par bande (dB), canaux additionnés.
    pub bands: Vec<f32>,
    /// Niveau de la partie stéréo (différence gauche / droite) par bande (dB).
    pub side: Vec<f32>,
    pub lufs: f64,
    pub peak_db: f64,
    pub rms_db: f64,
    /// Énergie stéréo relative au centre (0 % = mono).
    pub width_pct: f64,
}

/// Comparaison de l'original et du résultat sur le même extrait.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compare {
    /// Fréquence centrale de chaque bande (Hz).
    pub freqs: Vec<f32>,
    pub before: Analysis,
    pub after: Analysis,
    /// Gain appliqué avant le limiteur pour viser le volume cible sur l'extrait.
    pub gain_db: f64,
    /// Début et durée de l'extrait (secondes).
    pub from_s: f64,
    pub secs: f64,
}

fn spec_edges() -> Vec<f64> {
    (0..=SPEC_BANDS).map(|i| 30.0 * (18_000.0f64 / 30.0).powf(i as f64 / SPEC_BANDS as f64)).collect()
}

/// Extrait décodé en mémoire, toujours en stéréo entrelacée, précédé d'une seconde de
/// mise en route (pour que filtres et compresseur soient stabilisés). Renvoie
/// (échantillons, trames de mise en route, fréquence, début, durée).
fn excerpt(path: &Path, s: &Settings) -> Result<(Vec<f32>, usize, u32, f64, f64), String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as usize;
    if ch > 2 {
        return Err("seuls les fichiers mono et stéréo sont pris en charge".into());
    }
    let rate = dec.sample_rate;
    let total = dec.duration_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0);
    let a = s.start_s;
    let b = if s.end_s > 0.0 { s.end_s } else { total };
    let b = if b > a { b } else { a + SPEC_SECS };
    let len = (b - a).min(SPEC_SECS);
    let from = a + ((b - a) - len) / 2.0;
    let warm = from.min(1.0);
    let mut pos = 0.0;
    if from - warm > 0.5 {
        dec.seek_ms(((from - warm) * 1000.0) as u64)?;
        pos = from - warm;
    }
    let r = rate as f64;
    let skip = ((from - warm - pos).max(0.0) * r) as usize;
    let need = ((warm + len) * r) as usize;
    let mut out: Vec<f32> = Vec::with_capacity(need * 2);
    let (mut buf, mut seen) = (Vec::new(), 0usize);
    while out.len() < need * 2 && dec.next_block(&mut buf)? {
        for fr in buf.chunks(ch) {
            seen += 1;
            if seen <= skip {
                continue;
            }
            let l = fr[0];
            let rr = if ch == 2 { fr[1] } else { l };
            out.push(l);
            out.push(rr);
        }
    }
    out.truncate(need * 2);
    let warm_frames = ((warm * r) as usize).min(out.len() / 2);
    Ok((out, warm_frames, rate, from, len))
}

fn process_all(raw: &[f32], s: &Settings, rate: u32, gain_db: f64) -> Vec<f32> {
    let mut p = Processor::new(s, rate, 2, gain_db);
    let mut all = Vec::with_capacity(raw.len());
    let mut out = Vec::new();
    for b in raw.chunks(8192) {
        p.process(b, &mut out);
        all.extend_from_slice(&out);
    }
    p.flush(&mut out);
    all.extend_from_slice(&out);
    all
}

fn window_lufs(x: &[f32], rate: u32) -> Result<f64, String> {
    let mut m = EbuR128::new(2, rate, Mode::I).map_err(|e| e.to_string())?;
    m.add_frames_f32(x).map_err(|e| e.to_string())?;
    m.loudness_global().map_err(|e| e.to_string())
}

fn analyze(x: &[f32], rate: u32) -> Result<Analysis, String> {
    let edges = spec_edges();
    let r = rate as f64;
    // Bande de chaque case de la FFT (calculé une fois).
    let map: Vec<Option<usize>> = (0..SPEC_WIN / 2)
        .map(|k| {
            let fr = k as f64 * r / SPEC_WIN as f64;
            edges.windows(2).position(|w| fr >= w[0] && fr < w[1])
        })
        .collect();
    let mut bins = vec![0usize; SPEC_BANDS];
    for b in map.iter().flatten() {
        bins[*b] += 1;
    }
    let fft = FftPlanner::<f32>::new().plan_fft_forward(SPEC_WIN);
    let hann: Vec<f32> = (0..SPEC_WIN).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / SPEC_WIN as f32).cos()).collect();
    let frames = x.len() / 2;
    let (mut mid_acc, mut side_acc) = (vec![0f64; SPEC_BANDS], vec![0f64; SPEC_BANDS]);
    let mut count = 0usize;
    let mut cm = vec![Complex::new(0.0f32, 0.0); SPEC_WIN];
    let mut cs = cm.clone();
    let mut i = 0;
    while i + SPEC_WIN <= frames {
        for k in 0..SPEC_WIN {
            let (l, rr) = (x[(i + k) * 2], x[(i + k) * 2 + 1]);
            cm[k] = Complex::new((l + rr) * 0.5 * hann[k], 0.0);
            cs[k] = Complex::new((l - rr) * 0.5 * hann[k], 0.0);
        }
        fft.process(&mut cm);
        fft.process(&mut cs);
        for k in 0..SPEC_WIN / 2 {
            if let Some(b) = map[k] {
                mid_acc[b] += cm[k].norm_sqr() as f64;
                side_acc[b] += cs[k].norm_sqr() as f64;
            }
        }
        count += 1;
        i += SPEC_WIN / 2;
    }
    let db = |acc: &Vec<f64>| -> Vec<f32> {
        let mut v: Vec<Option<f32>> = acc
            .iter()
            .zip(&bins)
            .map(|(p, n)| (*n > 0 && count > 0).then(|| (10.0 * (p / *n as f64 / count as f64).max(1e-14).log10()) as f32))
            .collect();
        // Bande sans case (très basse fréquence à faible résolution) : valeur de la voisine.
        for j in 0..v.len() {
            if v[j].is_none() {
                v[j] = v[j + 1..].iter().flatten().next().copied().or_else(|| v[..j].iter().flatten().last().copied());
            }
        }
        v.into_iter().map(|o| o.unwrap_or(-140.0)).collect()
    };
    // Largeur : énergie stéréo / centre au-dessus de 150 Hz (sous 150 Hz, la grosse caisse et la
    // basse, au centre, fausseraient la mesure).
    let (mut pm, mut ps) = (0f64, 0f64);
    for (j, w) in edges.windows(2).enumerate() {
        if w[0] >= 150.0 {
            pm += mid_acc[j];
            ps += side_acc[j];
        }
    }
    let (mut peak, mut sq) = (0f32, 0f64);
    for fr in x.chunks(2) {
        let (l, rr) = (fr[0] as f64, fr[1] as f64);
        sq += l * l + rr * rr;
        peak = peak.max(fr[0].abs()).max(fr[1].abs());
    }
    let lufs = window_lufs(x, rate).unwrap_or(-70.0);
    Ok(Analysis {
        bands: db(&mid_acc),
        side: db(&side_acc),
        lufs: if lufs.is_finite() { lufs } else { -70.0 },
        peak_db: 20.0 * (peak.max(1e-9) as f64).log10(),
        rms_db: 10.0 * (sq / (2 * frames.max(1)) as f64).max(1e-14).log10(),
        width_pct: if pm > 0.0 { 100.0 * (ps / pm).sqrt() } else { 0.0 },
    })
}

/// Compare l'original et le résultat sur un extrait de 45 s au milieu de la partie gardée.
/// Rapide (l'extrait est décodé une fois), pour suivre les réglages en direct. Avec un volume
/// cible, le gain est cherché sur l'extrait lui-même (quelques passes en mémoire).
pub fn compare(path: &Path, s: &Settings) -> Result<Compare, String> {
    let s = s.clamped();
    let (raw, warm, rate, from, secs) = excerpt(path, &s)?;
    let cut = warm * 2;
    let before = analyze(&raw[cut..], rate)?;
    let mut gain = 0.0;
    if let Some(target) = s.target_lufs {
        let no_lim = Settings { limiter_on: false, ..s.clone() };
        let l0 = window_lufs(&process_all(&raw, &no_lim, rate, 0.0)[cut..], rate)?;
        if l0.is_finite() {
            let (mut g_prev, mut l_prev) = (target - l0, f64::NAN);
            let max_g = (g_prev + MAX_PUSH_DB).min(30.0);
            gain = g_prev;
            if s.limiter_on {
                for _ in 0..4 {
                    let l = window_lufs(&process_all(&raw, &s, rate, gain)[cut..], rate)?;
                    if !l.is_finite() || (l - target).abs() <= 0.3 {
                        break;
                    }
                    let slope = if l_prev.is_finite() && (l - l_prev).abs() > 0.05 { (gain - g_prev) / (l - l_prev) } else { 2.0 };
                    g_prev = gain;
                    l_prev = l;
                    let next = (gain + (target - l) * slope.clamp(1.0, 12.0)).clamp(-30.0, max_g);
                    if (next - gain).abs() < 0.01 {
                        break;
                    }
                    gain = next;
                }
            }
        }
    }
    let processed = process_all(&raw, &s, rate, gain);
    let after = analyze(&processed[cut.min(processed.len())..], rate)?;
    let edges = spec_edges();
    let freqs = edges.windows(2).map(|w| (w[0] * w[1]).sqrt() as f32).collect();
    Ok(Compare { freqs, before, after, gain_db: gain, from_s: from, secs })
}

// =====================================================================
// Écriture WAV 24 bits
// =====================================================================

pub struct Wav24 {
    w: BufWriter<File>,
    channels: u16,
    frames: u64,
}

impl Wav24 {
    pub fn create(path: &Path, rate: u32, channels: u16) -> std::io::Result<Wav24> {
        let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
        let block = channels as u32 * 3;
        w.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        w.write_all(&16u32.to_le_bytes())?;
        w.write_all(&1u16.to_le_bytes())?;
        w.write_all(&channels.to_le_bytes())?;
        w.write_all(&rate.to_le_bytes())?;
        w.write_all(&(rate * block).to_le_bytes())?;
        w.write_all(&(block as u16).to_le_bytes())?;
        w.write_all(&24u16.to_le_bytes())?;
        w.write_all(b"data\0\0\0\0")?;
        Ok(Wav24 { w, channels, frames: 0 })
    }

    pub fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) as f64 * 8_388_607.0).round() as i32;
            self.w.write_all(&v.to_le_bytes()[..3])?;
        }
        self.frames += (samples.len() / self.channels as usize) as u64;
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        let data = self.frames * self.channels as u64 * 3;
        if data > u32::MAX as u64 - 64 {
            return Err(std::io::Error::other("fichier WAV trop long (plus de 4 Go)"));
        }
        let pad = data % 2;
        if pad == 1 {
            self.w.write_all(&[0])?;
        }
        self.w.flush()?;
        let mut f = self.w.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&((36 + data + pad) as u32).to_le_bytes())?;
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&(data as u32).to_le_bytes())?;
        f.sync_all()
    }
}

// =====================================================================
// Export dans un nouveau fichier
// =====================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub track_id: i64,
    pub file_name: String,
    pub lufs: f64,
    pub peak_db: f64,
}

/// Réglages gardés par morceau (pour refaire l'export).
pub fn save_settings(conn: &Connection, id: i64, s: &Settings) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO remaster (track_id, settings) VALUES (?1, ?2)
         ON CONFLICT(track_id) DO UPDATE SET settings = excluded.settings, updated_at = unixepoch()",
        params![id, serde_json::to_string(s).unwrap_or_default()],
    )?;
    Ok(())
}

pub fn load_settings(conn: &Connection, id: i64) -> Result<Option<Settings>, AppError> {
    let raw: Option<String> = conn.query_row("SELECT settings FROM remaster WHERE track_id = ?1", [id], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
}

/// Exporte le morceau traité avec les réglages donnés dans un nouveau WAV, ajouté à la bibliothèque.
pub fn export(lib: &Library, conn: &Connection, id: i64, s: &Settings, batch: i64, task: Option<&TaskHandle>) -> Result<ExportResult, AppError> {
    let s = s.clamped();
    let (d, result) = crate::prep::derive::derive(lib, conn, id, "Remaster", "remaster", batch, |src, tmp| {
        let (rate, channels) = {
            let d = Decoder::open(src)?;
            (d.sample_rate, d.channels)
        };
        let gain = match s.target_lufs {
            Some(_) => prepare(src, &s)?.gain_db,
            None => 0.0,
        };
        // Écriture (le gain vient de la préparation, qui a déjà vérifié le volume obtenu).
        let mut w = Wav24::create(tmp, rate, channels).map_err(|e| e.to_string())?;
        let r = pass(src, &s, gain, |b| w.write(b).map_err(|e| e.to_string()), task)?;
        w.finish().map_err(|e| e.to_string())?;
        Ok(r)
    })?;
    save_settings(conn, id, &s)?;
    Ok(ExportResult { track_id: d.track_id, file_name: d.file_name, lufs: result.0, peak_db: result.1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::tests_support::library_on;

    /// Morceau synthétique stéréo (grosse caisse + nappe), assez faible (≈ −20 LUFS).
    fn track() -> Vec<i16> {
        let rate = 44_100.0;
        let mut s = Vec::new();
        for i in 0..(rate as usize * 20) {
            let t = i as f64 / rate;
            let beat = t % 0.5;
            let kick = 0.5 * (-beat * 14.0).exp() * (2.0 * std::f64::consts::PI * (55.0 + 80.0 * (-beat * 30.0).exp()) * beat).sin();
            let pad = 0.05 * (2.0 * std::f64::consts::PI * 440.0 * t).sin() + 0.03 * (2.0 * std::f64::consts::PI * 2500.0 * t).sin();
            let l = ((kick + pad) * 12000.0) as i16;
            let r = ((kick - pad) * 12000.0) as i16;
            s.push(l);
            s.push(r);
        }
        s
    }

    #[test]
    fn auto_atteint_le_volume_cible_sans_depasser_le_plafond() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        let src = crate::testdata::write(&music, "faible.wav", &crate::testdata::wav_from_samples(&track()));
        let original = std::fs::read(&src).unwrap();
        let m = measure(&src).unwrap();
        assert!(m.lufs < -15.0, "{m:?}");
        let s = auto_settings(&m, "club");
        assert_eq!(s.target_lufs, Some(-7.0));

        let (lib, ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let batch = crate::history::begin_batch(&conn, "history.remaster", None).unwrap();
        let r = export(&lib, &conn, ids[0], &s, batch, None).unwrap();
        assert!((r.lufs - (-7.0)).abs() <= 0.5, "volume obtenu {}", r.lufs);
        assert!(r.peak_db <= -1.0 + 0.01, "crête {}", r.peak_db);
        assert_eq!(r.file_name, "faible (Remaster).wav");
        assert_eq!(std::fs::read(&src).unwrap(), original, "original intact");

        // Nouveau fichier : WAV 24 bits lisible, dans la bibliothèque, version « Remaster ».
        let out = music.join("faible (Remaster).wav");
        let info = crate::tags::read::read_file(&out, &[]).unwrap();
        assert_eq!(info.bit_depth, Some(24));
        assert_eq!(info.tags.version.as_deref(), Some("Remaster"));
        let lufs = crate::djtools::loudness::measure(&out).unwrap().lufs;
        assert!((lufs - (-7.0)).abs() <= 0.5);
        assert!(load_settings(&conn, ids[0]).unwrap().is_some());

        // Annulation : le remaster part en quarantaine, l'original reste.
        let u = lib.apply_history(batch, true).unwrap();
        assert_eq!(u.ok, 1, "{:?}", u.failures);
        assert!(!out.exists());
        assert!(src.exists());
    }

    #[test]
    fn coupe_et_fondus_dans_le_traitement() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::testdata::write(dir.path(), "t.wav", &crate::testdata::wav_from_samples(&track()));
        // Coupe de 2,5 s à la fin (20 s) avec fondu de sortie : la longueur est comptée.
        let s = Settings { start_s: 2.5, fade_in_s: 1.0, fade_out_s: 2.0, target_lufs: None, ..Default::default() }.clamped();
        let mut all: Vec<f32> = Vec::new();
        pass(&p, &s, 0.0, |b| { all.extend_from_slice(b); Ok(()) }, None).unwrap();
        assert_eq!(all.len(), (17.5 * 44_100.0) as usize * 2, "20 s − 2,5 s");
        assert!(all[0].abs() < 1e-6 && all[1].abs() < 1e-6, "fondu d'entrée");
        let tail = &all[all.len() - 200..];
        assert!(tail.iter().all(|x| x.abs() < 0.01), "fondu de sortie");
        // Fin précisée : de 1,0 s à 5,0 s.
        let s = Settings { start_s: 1.0, end_s: 5.0, target_lufs: None, ..Default::default() };
        let mut n = 0usize;
        pass(&p, &s, 0.0, |b| { n += b.len(); Ok(()) }, None).unwrap();
        assert_eq!(n, 4 * 44_100 * 2);
    }

    #[test]
    fn spectre_avant_apres() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::testdata::write(dir.path(), "t.wav", &crate::testdata::wav_from_samples(&track()));
        let mut s = Settings { limiter_on: false, highpass_hz: 0.0, ..Default::default() };
        s.bands[3].gain_db = 6.0; // 3,5 kHz
        let c = compare(&p, &s).unwrap();
        assert_eq!(c.freqs.len(), SPEC_BANDS);
        assert_eq!(c.before.bands.len(), SPEC_BANDS);
        let band = |hz: f32| c.freqs.iter().position(|f| *f >= hz).unwrap();
        // La nappe à 2,5 kHz monte, les graves (kick ~60 Hz) bougent peu.
        let (h, b) = (band(2500.0), band(60.0));
        assert!(c.after.bands[h] - c.before.bands[h] > 2.0, "{} {}", c.after.bands[h], c.before.bands[h]);
        assert!((c.after.bands[b] - c.before.bands[b]).abs() < 1.0);
        // Extrait : 20 s de morceau, donc tout le morceau.
        assert!((c.secs - 20.0).abs() < 0.1 && c.from_s.abs() < 0.1, "{} {}", c.from_s, c.secs);

        // Volume cible atteint sur l'extrait (qui commence après la coupe), crête sous le plafond.
        let s = Settings { target_lufs: Some(-10.0), start_s: 5.0, ..Default::default() };
        let c = compare(&p, &s).unwrap();
        assert!((c.after.lufs + 10.0).abs() < 0.6, "{}", c.after.lufs);
        assert!(c.after.peak_db <= -0.99, "{}", c.after.peak_db);
        assert!((c.from_s - 5.0).abs() < 0.1 && (c.secs - 15.0).abs() < 0.1);
        // Tout en mono : plus d'énergie stéréo.
        let c = compare(&p, &Settings { width: 0.0, ..Default::default() }).unwrap();
        assert!(c.before.width_pct > 5.0 && c.after.width_pct < 1.0, "{} {}", c.before.width_pct, c.after.width_pct);
    }
}

/// Essai sur la COPIE d'un vrai fichier (l'original est seulement lu) :
/// `DJLM_DIAG_FILE=E:\...\x.mp3 cargo test --release remaster_reel -- --ignored --nocapture`
#[cfg(test)]
mod reel {
    #[test]
    #[ignore]
    fn remaster_reel() {
        let Ok(src) = std::env::var("DJLM_DIAG_FILE") else { return };
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        let name = std::path::Path::new(&src).file_name().unwrap();
        std::fs::copy(&src, music.join(name)).unwrap();
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let t0 = std::time::Instant::now();
        let m = super::measure(&music.join(name)).unwrap();
        let s = super::auto_settings(&m, "auto");
        for (k, s) in [("neutre", super::Settings::default()), ("mono120", super::Settings { mono_bass_hz: 120.0, ..Default::default() }), ("auto", s.clone()), ("sans-comp", super::Settings { comp_on: false, ..s.clone() }), ("sans-volume", super::Settings { target_lufs: None, ..s.clone() }), ("sans-eq", super::Settings { eq_on: false, ..s.clone() })] {
            print!("{k} ");
            let t1 = std::time::Instant::now();
            let c = super::compare(&music.join(name), &s).unwrap();
            { let (raw, _, rate, _, _) = super::excerpt(&music.join(name), &s.clamped()).unwrap(); let out = super::process_all(&raw, &s.clamped(), rate, c.gain_db); let ceil = 10f32.powf(s.ceiling_db as f32 / 20.0); let n = out.iter().filter(|x| x.abs() >= ceil * 0.9999).count(); println!("REEL gain {:.1} dB, échantillons au plafond {} sur {}", c.gain_db, n, out.len()); }
            println!("REEL comparaison {:.2} s : {:.1} -> {:.1} LUFS, largeur {:.0} -> {:.0} %", t1.elapsed().as_secs_f64(), c.before.lufs, c.after.lufs, c.before.width_pct, c.after.width_pct);
        }
        println!("REEL mesures {:.1} LUFS, crête {:.1}, équilibre {:?} ({:.1} s)", m.lufs, m.peak_db, m.balance.map(|x| (x * 10.0).round() / 10.0), t0.elapsed().as_secs_f64());
        println!("REEL réglages EQ {:?} comp {} {:.1}:1 seuil {:.1}", s.bands.iter().map(|b| b.gain_db).collect::<Vec<_>>(), s.comp_on, s.ratio, s.threshold_db);
        let t1 = std::time::Instant::now();
        let batch = crate::history::begin_batch(&conn, "history.remaster", None).unwrap();
        let r = super::export(&lib, &conn, ids[0], &s, batch, None).unwrap();
        println!("REEL export {} : {:.2} LUFS, crête {:.2} dB en {:.1} s", r.file_name, r.lufs, r.peak_db, t1.elapsed().as_secs_f64());
    }
}

#[cfg(test)]
mod calibrage {
    /// Équilibre médian d'un échantillon d'un vrai dossier (lecture seule).
    #[test]
    #[ignore]
    fn equilibre_median() {
        let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
        let step: usize = std::env::var("DJLM_DIAG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(40);
        let mut cols: Vec<Vec<f64>> = vec![vec![]; 5];
        let mut lufs = vec![];
        for (i, e) in std::fs::read_dir(dir).unwrap().flatten().enumerate() {
            let p = e.path();
            if i % step != 0 || !crate::tags::is_audio_path(&p) {
                continue;
            }
            if let Ok(m) = super::measure(&p) {
                for k in 0..5 {
                    cols[k].push(m.balance[k]);
                }
                lufs.push(m.lufs);
            }
        }
        let med = |v: &mut Vec<f64>| {
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() / 2]
        };
        let b: Vec<f64> = cols.iter_mut().map(|c| (med(c) * 10.0).round() / 10.0).collect();
        println!("CALIB {} morceaux, équilibre médian {:?}, volume médian {:.1} LUFS", lufs.len(), b, med(&mut lufs));
    }
}
