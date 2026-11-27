//! Analyse du BPM, de la tonalité et de l'énergie, sur l'ordinateur.
//!
//! Pour les morceaux qui n'ont pas encore de BPM ou de tonalité (Rekordbox reste la
//! référence : une valeur existante n'est jamais remplacée).
//! - BPM : attaques des graves (grosse caisse), autocorrélation, peigne sur 1, 2 et 4
//!   périodes, période la plus rapide bien marquée, ramenée entre 78 et 185 BPM.
//! - Tonalité : chromagramme (énergie de chacune des 12 notes) comparé aux profils de
//!   Krumhansl (24 tonalités majeures et mineures).
//! - Énergie (1 à 10) : volume moyen, brillance (centroïde spectral) et densité d'attaques.
//! Le son est lu seulement ; rien n'est écrit sans l'accord de l'utilisateur.

use std::path::Path;

use rusqlite::{params, Connection};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;

use crate::audio::decode::Decoder;
use crate::error::AppError;
use crate::library::edit::{EditReport, Source};
use crate::library::{volumes, Library};
use crate::tags::write::Changes;
use crate::tasks::TaskHandle;

const NOTES: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const MAJOR: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub bpm: Option<f64>,
    /// Notation musicale (« Am », « F# »).
    pub key: Option<String>,
    pub energy: Option<i64>,
}

/// Son en mono, ramené vers 11 kHz environ (moyenne de `factor` échantillons), limité à
/// `max_s` secondes après `skip_s` secondes (les intros sont souvent pauvres).
fn mono(path: &Path, skip_s: f64, max_s: f64) -> Result<(Vec<f32>, f64), String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as usize;
    let factor = ((dec.sample_rate as f64 / 11_025.0).round() as usize).max(1);
    let rate = dec.sample_rate as f64 / factor as f64;
    let skip = (skip_s * dec.sample_rate as f64) as usize;
    let max = (max_s * rate) as usize;
    let mut out = Vec::with_capacity(max);
    let (mut acc, mut n, mut frame_i) = (0f32, 0usize, 0usize);
    let mut buf = Vec::new();
    while dec.next_block(&mut buf)? {
        for f in buf.chunks(ch) {
            frame_i += 1;
            if frame_i <= skip {
                continue;
            }
            acc += f.iter().sum::<f32>() / ch as f32;
            n += 1;
            if n == factor {
                out.push(acc / factor as f32);
                acc = 0.0;
                n = 0;
                if out.len() >= max {
                    return Ok((out, rate));
                }
            }
        }
    }
    Ok((out, rate))
}

fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos()).collect()
}

/// Spectres d'amplitude (fenêtre `win`, pas `hop`).
fn spectra(x: &[f32], win: usize, hop: usize) -> Vec<Vec<f32>> {
    let fft = FftPlanner::<f32>::new().plan_fft_forward(win);
    let w = hann(win);
    let mut out = Vec::new();
    let mut pos = 0;
    let mut buf = vec![Complex::new(0.0, 0.0); win];
    while pos + win <= x.len() {
        for i in 0..win {
            buf[i] = Complex::new(x[pos + i] * w[i], 0.0);
        }
        fft.process(&mut buf);
        out.push(buf[..win / 2].iter().map(|c| c.norm()).collect());
        pos += hop;
    }
    out
}

/// BPM d'après l'enveloppe des attaques (`fps` : images par seconde).
pub fn bpm_from_onsets(env: &[f32], fps: f64) -> Option<f64> {
    if env.len() < (fps * 8.0) as usize {
        return None;
    }
    // Retire la moyenne glissante (0,5 s) : ne garde que les attaques.
    let w = (fps * 0.5) as usize;
    let mut o: Vec<f64> = Vec::with_capacity(env.len());
    for i in 0..env.len() {
        let a = i.saturating_sub(w);
        let b = (i + w).min(env.len());
        let mean = env[a..b].iter().map(|v| *v as f64).sum::<f64>() / (b - a) as f64;
        o.push((env[i] as f64 - mean).max(0.0));
    }
    let max_lag = (fps * 60.0 / 40.0) as usize * 4 + 2;
    let acf: Vec<f64> = (0..max_lag.min(o.len() - 1)).map(|l| o.iter().zip(o.iter().skip(l)).map(|(a, b)| a * b).sum::<f64>()).collect();
    let at = |lag: f64| -> f64 {
        let i = lag.floor() as usize;
        if i + 1 >= acf.len() {
            return 0.0;
        }
        let f = lag - i as f64;
        acf[i] * (1.0 - f) + acf[i + 1] * f
    };
    let norm = acf.first().copied().unwrap_or(1.0).max(1e-9);
    // Peigne sur 1, 2 et 4 périodes (puissances de deux seulement : 3 périodes ferait gagner
    // les 2/3 du tempo), de 60 à 400 « BPM ».
    let comb = |bpm: f64| {
        let lag = 60.0 * fps / bpm;
        (at(lag) + 0.5 * at(lag * 2.0) + 0.25 * at(lag * 4.0)) / norm
    };
    let mut scores = Vec::new();
    let mut bpm = 60.0;
    while bpm <= 400.0 {
        scores.push((bpm, comb(bpm)));
        bpm += 0.05;
    }
    let max = scores.iter().map(|s| s.1).fold(0.0, f64::max);
    if max < 0.05 {
        return None; // pas de régularité nette
    }
    // Période fondamentale : la plus rapide qui atteint presque le meilleur score (un rythme
    // régulier à la noire l'est aussi à la blanche) ; affinée au maximum local.
    let first = scores.iter().rposition(|s| s.1 >= 0.92 * max)?;
    let mut i = first;
    while i > 0 && scores[i - 1].1 >= scores[i].1 {
        i -= 1;
    }
    while i + 1 < scores.len() && scores[i + 1].1 > scores[i].1 {
        i += 1;
    }
    // Ramené dans l'intervalle habituel par doublements et moitiés seulement.
    let mut b = scores[i].0;
    while b < 78.0 {
        b *= 2.0;
    }
    while b > 185.0 {
        b /= 2.0;
    }
    // Valeur entière quand elle en est proche (la plupart des morceaux de club).
    let r = b.round();
    Some(if (b - r).abs() < 0.25 { r } else { (b * 10.0).round() / 10.0 })
}

/// Tonalité d'après un chromagramme (C = 0).
pub fn key_from_chroma(chroma: &[f64; 12]) -> Option<String> {
    let total: f64 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let corr = |profile: &[f64; 12], root: usize| -> f64 {
        let x: Vec<f64> = (0..12).map(|i| chroma[(i + root) % 12]).collect();
        let (mx, my) = (x.iter().sum::<f64>() / 12.0, profile.iter().sum::<f64>() / 12.0);
        let (mut num, mut dx, mut dy) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            num += (x[i] - mx) * (profile[i] - my);
            dx += (x[i] - mx).powi(2);
            dy += (profile[i] - my).powi(2);
        }
        num / (dx.sqrt() * dy.sqrt()).max(1e-12)
    };
    let mut best = (f64::MIN, String::new());
    for root in 0..12 {
        for (profile, minor) in [(&MAJOR, false), (&MINOR, true)] {
            let c = corr(profile, root);
            if c > best.0 {
                best = (c, format!("{}{}", NOTES[root], if minor { "m" } else { "" }));
            }
        }
    }
    Some(best.1)
}

/// Analyse d'un fichier.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let (x, rate) = mono(path, 15.0, 120.0)?;
    let (x, rate) = if x.len() < (rate * 20.0) as usize { mono(path, 0.0, 120.0)? } else { (x, rate) };
    if x.len() < (rate * 8.0) as usize {
        return Err("morceau trop court pour l'analyse".into());
    }
    // --- Attaques (BPM, densité) ---
    let hop = 128;
    let spec = spectra(&x, 512, hop);
    let fps = rate / hop as f64;
    let logs: Vec<Vec<f32>> = spec.iter().map(|s| s.iter().map(|m| (1.0 + 100.0 * m).ln()).collect()).collect();
    // Deux enveloppes d'attaques : graves seuls (grosse caisse, sous 150 Hz), qui donnent le
    // temps ; tout le spectre en secours (morceaux sans grosse caisse). Sinon une charleston
    // sur les contretemps rend le tempo ambigu (double, 2/3…).
    let low = ((150.0 * 512.0 / rate) as usize).max(1);
    let mut env = vec![0f32; logs.len()];
    let mut env_low = vec![0f32; logs.len()];
    for t in 1..logs.len() {
        for (k, (a, b)) in logs[t].iter().zip(&logs[t - 1]).enumerate() {
            let d = (a - b).max(0.0);
            env[t] += d;
            if k <= low {
                env_low[t] += d;
            }
        }
    }
    let bpm = bpm_from_onsets(&env_low, fps).or_else(|| bpm_from_onsets(&env, fps));

    // --- Tonalité (chromagramme entre 65 Hz et 2,1 kHz) ---
    let win = 4096;
    let kspec = spectra(&x, win, 2048);
    let mut chroma = [0f64; 12];
    for s in &kspec {
        let mut frame = [0f64; 12];
        for (k, m) in s.iter().enumerate().skip(1) {
            let f = k as f64 * rate / win as f64;
            if !(65.0..=2100.0).contains(&f) {
                continue;
            }
            let midi = 69.0 + 12.0 * (f / 440.0).log2();
            frame[(midi.round() as i64).rem_euclid(12) as usize] += *m as f64;
        }
        let mx = frame.iter().cloned().fold(0.0, f64::max);
        if mx > 0.0 {
            for i in 0..12 {
                chroma[i] += frame[i] / mx;
            }
        }
    }
    let key = key_from_chroma(&chroma);

    // --- Énergie (1 à 10) ---
    let rms = (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
    let rms_db = 20.0 * rms.max(1e-9).log10();
    let mut centroid = 0.0;
    for s in &spec {
        let tot: f32 = s.iter().sum();
        if tot > 0.0 {
            centroid += s.iter().enumerate().map(|(k, m)| k as f64 * *m as f64).sum::<f64>() / tot as f64 * rate / 512.0;
        }
    }
    centroid /= spec.len().max(1) as f64;
    let mean_env = env.iter().map(|v| *v as f64).sum::<f64>() / env.len().max(1) as f64;
    let n = |v: f64, lo: f64, hi: f64| ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let raw = 0.55 * n(rms_db, -30.0, -8.0) + 0.25 * n(centroid, 600.0, 2600.0) + 0.20 * n(mean_env, 5.0, 60.0);
    let energy = Some(1 + (raw * 9.0).round() as i64);
    Ok(Analysis { bpm, key, energy })
}

/// Morceaux à analyser : sans BPM ou sans tonalité (ou `ids`), sans résultat à jour.
fn pending(conn: &Connection, ids: Option<&[i64]>) -> Result<Vec<(i64, i64, String, i64, i64)>, AppError> {
    let mut st = conn.prepare(
        "SELECT t.id, t.volume_id, t.rel_path, t.size, t.mtime FROM tracks t
         LEFT JOIN analysis a ON a.track_id = t.id
         WHERE t.status = 'ok' AND (a.track_id IS NULL OR a.size != t.size OR a.mtime != t.mtime)
           AND (?1 OR coalesce(t.bpm, 0) = 0 OR coalesce(t.musical_key, '') = '')",
    )?;
    let explicit = ids.is_some();
    let rows: Vec<(i64, i64, String, i64, i64)> = st.query_map([explicit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    Ok(match ids {
        Some(ids) => rows.into_iter().filter(|r| ids.contains(&r.0)).collect(),
        None => rows,
    })
}

/// Analyse (tâche de fond annulable) ; résultats gardés dans la base, rien n'est écrit.
pub fn run(lib: &Library, conn: &Connection, ids: Option<&[i64]>, task: Option<&TaskHandle>) -> Result<usize, AppError> {
    let todo = pending(conn, ids)?;
    if let Some(t) = task {
        t.set_total(todo.len() as u64);
    }
    let mounts = lib.mounts();
    let mut done = 0;
    for (i, (id, vol, rel, size, mtime)) in todo.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        if let Some(m) = mounts.get(vol) {
            let a = analyze(&volumes::join_rel(m, rel)).unwrap_or_else(|e| {
                log::info!("Analyse impossible ({rel}) : {e}");
                Analysis { bpm: None, key: None, energy: None }
            });
            conn.execute(
                "INSERT INTO analysis (track_id, size, mtime, bpm, musical_key, energy) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, bpm = excluded.bpm,
                   musical_key = excluded.musical_key, energy = excluded.energy, analyzed_at = unixepoch()",
                params![id, size, mtime, a.bpm, a.key, a.energy],
            )?;
            done += 1;
        }
        if let Some(t) = task {
            t.progress(i as u64 + 1);
        }
    }
    lib.notify_changed(true);
    Ok(done)
}

/// Résultat proposé pour un morceau (aperçu avant écriture).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub track_id: i64,
    pub title: String,
    pub current_bpm: Option<f64>,
    pub current_key: Option<String>,
    /// Proposés seulement si le morceau n'a pas de valeur.
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub energy: Option<i64>,
}

pub fn proposals(conn: &Connection) -> Result<Vec<Proposal>, AppError> {
    let mut st = conn.prepare(
        "SELECT t.id, coalesce(t.main_artist || ' - ', '') || coalesce(t.title, t.file_name), t.bpm, t.musical_key, a.bpm, a.musical_key, a.energy
         FROM analysis a JOIN tracks t ON t.id = a.track_id
         WHERE t.status = 'ok' AND ((coalesce(t.bpm, 0) = 0 AND a.bpm IS NOT NULL) OR (coalesce(t.musical_key, '') = '' AND a.musical_key IS NOT NULL))
         ORDER BY 2 COLLATE NOCASE",
    )?;
    let rows = st.query_map([], |r| {
        let cur_bpm: Option<f64> = r.get(2)?;
        let cur_key: Option<String> = r.get(3)?;
        let bpm: Option<f64> = r.get(4)?;
        let key: Option<String> = r.get(5)?;
        Ok(Proposal {
            track_id: r.get(0)?,
            title: r.get(1)?,
            bpm: bpm.filter(|_| cur_bpm.unwrap_or(0.0) == 0.0),
            key: key.filter(|_| cur_key.as_deref().unwrap_or("").is_empty()),
            current_bpm: cur_bpm,
            current_key: cur_key,
            energy: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Écrit les BPM et tonalités proposés (modification automatique : les champs verrouillés
/// ne sont pas touchés ; un seul lot d'historique).
pub fn apply(lib: &Library, conn: &Connection, ids: &[i64], task: Option<&TaskHandle>) -> Result<EditReport, AppError> {
    let items: Vec<(i64, Changes)> = proposals(conn)?
        .into_iter()
        .filter(|p| ids.contains(&p.track_id))
        .map(|p| (p.track_id, Changes { bpm: p.bpm.map(Some), key: p.key.map(Some), ..Default::default() }))
        .collect();
    lib.edit_each(items, Source::Automatic, "history.analysis", task)
}

#[cfg(test)]
pub(crate) fn tests_kicks(bpm: f64, secs: f64) -> Vec<f64> {
    tests::kicks(bpm, secs)
}
#[cfg(test)]
pub(crate) fn tests_write(dir: &Path, name: &str, mono: &[f64]) -> std::path::PathBuf {
    tests::write_stereo(dir, name, mono)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 44_100.0;

    pub(super) fn write_stereo(dir: &Path, name: &str, mono: &[f64]) -> std::path::PathBuf {
        let mut s: Vec<i16> = Vec::with_capacity(mono.len() * 2);
        for v in mono {
            let x = (v.clamp(-1.0, 1.0) * 30000.0) as i16;
            s.push(x);
            s.push(x);
        }
        crate::testdata::write(dir, name, &crate::testdata::wav_from_samples(&s))
    }

    /// Grosse caisse synthétique (sinus descendant amorti) à `bpm` pendant `secs` secondes.
    pub(super) fn kicks(bpm: f64, secs: f64) -> Vec<f64> {
        let n = (RATE * secs) as usize;
        let period = 60.0 / bpm;
        let mut x = vec![0.0; n];
        let mut seed = 1u32;
        for (i, v) in x.iter_mut().enumerate() {
            let t = i as f64 / RATE;
            let tb = t % period;
            let f = 50.0 + 120.0 * (-tb * 30.0).exp();
            *v = 0.8 * (-tb * 12.0).exp() * (2.0 * std::f64::consts::PI * f * tb).sin();
            // Charleston sur les contretemps, léger bruit.
            let th = (t + period / 2.0) % period;
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v += 0.15 * (-th * 60.0).exp() * ((seed >> 8) as f64 / (1u32 << 24) as f64 - 0.5);
        }
        x
    }

    #[test]
    fn bpm_de_battements_synthetiques() {
        let dir = tempfile::tempdir().unwrap();
        for bpm in [124.0, 128.0, 140.0, 174.0, 90.0] {
            let p = write_stereo(dir.path(), &format!("k{bpm}.wav"), &kicks(bpm, 40.0));
            let a = analyze(&p).unwrap();
            let got = a.bpm.unwrap();
            assert!((got - bpm).abs() <= 1.0, "attendu {bpm}, trouvé {got}");
            assert!(a.energy.unwrap() >= 1 && a.energy.unwrap() <= 10);
        }
    }

    #[test]
    fn tonalite_d_accords_synthetiques() {
        let dir = tempfile::tempdir().unwrap();
        // La mineur : accords Am, Dm, Em, Am (fondamentale + harmoniques), 32 s.
        let chords: [&[f64]; 4] = [&[57.0, 60.0, 64.0, 45.0], &[62.0, 65.0, 69.0, 50.0], &[64.0, 67.0, 71.0, 52.0], &[57.0, 60.0, 64.0, 45.0]];
        let mut x = Vec::new();
        for c in chords.iter().cycle().take(8) {
            for i in 0..(RATE * 4.0) as usize {
                let t = i as f64 / RATE;
                let mut v = 0.0;
                for midi in c.iter() {
                    let f = 440.0 * 2f64.powf((midi - 69.0) / 12.0);
                    for h in 1..4 {
                        v += 0.12 / h as f64 * (2.0 * std::f64::consts::PI * f * h as f64 * t).sin();
                    }
                }
                x.push(v);
            }
        }
        let p = write_stereo(dir.path(), "am.wav", &x);
        assert_eq!(analyze(&p).unwrap().key.as_deref(), Some("Am"));
    }

    #[test]
    fn profils_de_krumhansl() {
        let mut c = [0.0; 12];
        for (i, v) in MAJOR.iter().enumerate() {
            c[(i + 7) % 12] = *v; // G majeur
        }
        assert_eq!(key_from_chroma(&c).as_deref(), Some("G"));
        assert_eq!(key_from_chroma(&[0.0; 12]), None);
    }
}


/// Comparaison sur un vrai dossier (lecture seule) avec les BPM et tonalités des tags :
/// `DJLM_DIAG_DIR=E:\... DJLM_DIAG_STEP=60 cargo test --release comparaison_reelle -- --ignored --nocapture`
#[cfg(test)]
mod reel {
    #[test]
    #[ignore]
    fn comparaison_reelle() {
        let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
        let step: usize = std::env::var("DJLM_DIAG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(50);
        let (mut n, mut bpm_ok, mut bpm_oct, mut key_ok, mut key_compat, mut keys) = (0, 0, 0, 0, 0, 0);
        for (i, e) in std::fs::read_dir(dir).unwrap().flatten().enumerate() {
            let p = e.path();
            if i % step != 0 || !crate::tags::is_audio_path(&p) {
                continue;
            }
            let Ok(info) = crate::tags::read::read_file(&p, &[]) else { continue };
            let Ok(a) = super::analyze(&p) else { continue };
            if let (Some(t), Some(g)) = (info.tags.bpm, a.bpm) {
                n += 1;
                if (t - g).abs() <= 1.0 {
                    bpm_ok += 1;
                } else if (t - g * 2.0).abs() <= 2.0 || (t - g / 2.0).abs() <= 1.0 {
                    bpm_oct += 1;
                } else {
                    println!("BPM {:>6.1} trouvé {:>6.1}  {}", t, g, p.file_name().unwrap().to_string_lossy());
                }
            }
            if let (Some(t), Some(g)) = (info.tags.key.as_deref(), a.key.as_deref()) {
                keys += 1;
                match crate::djtools::key_compat(t, g) {
                    "same" => key_ok += 1,
                    "adjacent" | "relative" => key_compat += 1,
                    _ => println!("TON {t:>4} trouvé {g:>4}  {}", p.file_name().unwrap().to_string_lossy()),
                }
            }
        }
        println!("RESULTAT BPM {bpm_ok}/{n} exacts, {bpm_oct} au double/moitié ; TONALITE {key_ok}/{keys} exactes, {key_compat} voisines");
    }
}
