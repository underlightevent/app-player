//! Structure d'un morceau (phase 8) : grille des temps, sections, repères proposés,
//! silences au début et à la fin, « couleur » du son (profil spectral pour les morceaux
//! similaires).
//!
//! Méthode, simple et prévisible :
//! - enveloppe d'énergie des graves (grosse caisse) toutes les ~5,8 ms ;
//! - tempo : celui du tag s'il existe (Rekordbox est la référence), sinon l'analyse de la
//!   phase 6 ; affiné à ±0,3 BPM et calé (phase) sur les attaques de tout le morceau ;
//! - premier temps fort : premier temps où le son commence (la musique de club démarre
//!   presque toujours sur le premier temps d'une mesure) ;
//! - sections par blocs de 8 mesures selon l'énergie globale et celle des graves :
//!   intro, montée, drop, break, outro ;
//! - repères : début, premier drop, premier break après le drop, début de l'outro.
//! Le son est seulement lu ; rien n'est écrit dans les fichiers.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};

use crate::audio::decode::Decoder;
use crate::error::AppError;
use crate::library::{volumes, Library};
use crate::tasks::TaskHandle;

/// Seuil du silence (dBFS, moyenne sur ~6 ms).
const SILENCE_DB: f64 = -50.0;
/// Bandes du profil spectral (de 40 Hz à 16 kHz).
pub const TIMBRE_BANDS: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// « intro », « build », « drop », « break », « outro », « main »
    pub kind: String,
    pub start_ms: f64,
    pub end_ms: f64,
    pub start_bar: usize,
    pub bars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    /// Repère direct (« hot cue ») : 0 = A, 1 = B… ; `None` = repère mémoire.
    pub hot: Option<u8>,
    /// Nom affiché (« Début », « Drop »…), clé de traduction côté interface.
    pub name: String,
    pub ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    pub bpm: f64,
    /// Position du premier temps fort (ms) ; la grille s'étend avant et après.
    pub first_beat_ms: f64,
    pub bars: usize,
    pub sections: Vec<Section>,
    pub cues: Vec<Cue>,
    pub lead_silence_ms: f64,
    pub tail_silence_ms: f64,
    pub duration_ms: f64,
    /// Profil spectral (dB relatifs, 12 bandes).
    pub timbre: Vec<f32>,
    /// Netteté de la grille (0 à 1) : faible = morceau sans rythme régulier.
    pub confidence: f64,
}

impl Structure {
    pub fn beat_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }
    pub fn bar_ms(&self) -> f64 {
        self.beat_ms() * 4.0
    }
}

/// Mesures brutes toutes les `hop` trames.
struct Features {
    fps: f64,
    /// Énergie des graves (< 150 Hz), linéaire.
    low: Vec<f32>,
    /// Énergie totale, linéaire.
    full: Vec<f32>,
    timbre: Vec<f32>,
    duration_ms: f64,
}

fn features(path: &Path) -> Result<Features, String> {
    let mut dec = Decoder::open(path)?;
    let ch = dec.channels.max(1) as usize;
    let rate = dec.sample_rate as f64;
    let hop = ((rate / 172.0).round() as usize).max(1);
    // Passe-bas 150 Hz (deux pôles simples en cascade).
    let a = (-2.0 * std::f64::consts::PI * 150.0 / rate).exp();
    let (mut l1, mut l2) = (0f64, 0f64);
    let (mut acc_low, mut acc_full, mut n) = (0f64, 0f64, 0usize);
    let (mut low, mut full) = (Vec::new(), Vec::new());
    // Profil spectral : une fenêtre de 4096 toutes les ~2 s.
    let win = 4096;
    let every = (rate * 2.0) as usize;
    let fft = FftPlanner::<f32>::new().plan_fft_forward(win);
    let hann: Vec<f32> = (0..win).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / win as f32).cos()).collect();
    let edges: Vec<f64> = (0..=TIMBRE_BANDS).map(|i| 40.0 * (16_000.0f64 / 40.0).powf(i as f64 / TIMBRE_BANDS as f64)).collect();
    let band_of: Vec<Option<usize>> = (0..win / 2).map(|k| {
        let f = k as f64 * rate / win as f64;
        edges.windows(2).position(|w| f >= w[0] && f < w[1])
    }).collect();
    let mut tim = vec![0f64; TIMBRE_BANDS];
    let mut ring: Vec<f32> = Vec::with_capacity(win);
    let mut cbuf = vec![Complex::new(0.0f32, 0.0); win];
    let (mut total, mut since) = (0usize, 0usize);
    let mut buf = Vec::new();
    while dec.next_block(&mut buf)? {
        for f in buf.chunks(ch) {
            let m = (f.iter().sum::<f32>() / f.len() as f32) as f64;
            l1 = m + a * (l1 - m);
            l2 = l1 + a * (l2 - l1);
            acc_low += l2 * l2;
            acc_full += m * m;
            n += 1;
            if n == hop {
                low.push((acc_low / hop as f64) as f32);
                full.push((acc_full / hop as f64) as f32);
                acc_low = 0.0;
                acc_full = 0.0;
                n = 0;
            }
            total += 1;
            since += 1;
            if since >= every || !ring.is_empty() {
                ring.push(m as f32);
                if ring.len() == win {
                    for i in 0..win {
                        cbuf[i] = Complex::new(ring[i] * hann[i], 0.0);
                    }
                    fft.process(&mut cbuf);
                    for (k, b) in band_of.iter().enumerate() {
                        if let Some(b) = b {
                            tim[*b] += cbuf[k].norm_sqr() as f64;
                        }
                    }
                    ring.clear();
                    since = 0;
                }
            }
        }
    }
    // Profil relatif à son maximum (indépendant du volume).
    let counts: Vec<usize> = (0..TIMBRE_BANDS).map(|b| band_of.iter().filter(|x| **x == Some(b)).count().max(1)).collect();
    let db: Vec<f64> = tim.iter().zip(&counts).map(|(p, c)| 10.0 * (p / *c as f64).max(1e-20).log10()).collect();
    let mx = db.iter().cloned().fold(f64::MIN, f64::max);
    Ok(Features {
        fps: rate / hop as f64,
        low,
        full,
        timbre: db.iter().map(|d| (d - mx) as f32).collect(),
        duration_ms: total as f64 * 1000.0 / rate,
    })
}

fn db(x: f32) -> f64 {
    10.0 * (x as f64).max(1e-12).log10()
}

/// Attaques des graves : hausse d'énergie d'une image à l'autre (en dB, positive seulement).
fn onsets(low: &[f32]) -> Vec<f32> {
    let mut o = vec![0f32; low.len()];
    for i in 1..low.len() {
        o[i] = (db(low[i]) - db(low[i - 1])).max(0.0) as f32;
    }
    o
}

/// Score d'alignement d'une grille (période `p` images, phase `ph`) sur les attaques.
fn grid_score(o: &[f32], p: f64, ph: f64) -> f64 {
    let mut s = 0f64;
    let mut t = ph;
    while (t as usize) + 1 < o.len() {
        let i = t.round() as usize;
        // Tolérance d'une image de chaque côté.
        let v = o[i].max(o[i.saturating_sub(1)]).max(o[(i + 1).min(o.len() - 1)]);
        s += v as f64;
        t += p;
    }
    s
}

/// Tempo affiné et phase de la grille (en images).
fn fit_grid(o: &[f32], fps: f64, bpm0: f64) -> (f64, f64, f64) {
    let mut best = (bpm0, 0.0, f64::MIN);
    let search = |best: &mut (f64, f64, f64), bpm: f64, step_ph: f64| {
        let p = 60.0 * fps / bpm;
        let mut ph = 0.0;
        while ph < p {
            let sc = grid_score(o, p, ph);
            if sc > best.2 {
                *best = (bpm, ph, sc);
            }
            ph += step_ph;
        }
    };
    // Grossier puis fin.
    let mut b = bpm0 - 0.3;
    while b <= bpm0 + 0.3 + 1e-9 {
        search(&mut best, b, 1.0);
        b += 0.05;
    }
    let (b1, _, _) = best;
    let mut b = b1 - 0.05;
    while b <= b1 + 0.05 + 1e-9 {
        search(&mut best, b, 0.25);
        b += 0.01;
    }
    // Netteté : score de la grille / score d'une grille décalée d'un demi-temps.
    let p = 60.0 * fps / best.0;
    let off = grid_score(o, p, (best.1 + p / 2.0) % p).max(1e-9);
    let conf = ((best.2 / off - 1.0) / 2.0).clamp(0.0, 1.0);
    (best.0, best.1, conf)
}

/// Analyse complète d'un fichier. `tag_bpm` : BPM du tag (référence) s'il existe.
pub fn analyze(path: &Path, tag_bpm: Option<f64>) -> Result<Structure, String> {
    let f = features(path)?;
    let n = f.full.len();
    if n < (f.fps * 10.0) as usize {
        return Err("morceau trop court".into());
    }
    let ms = |i: f64| i * 1000.0 / f.fps;
    // Silences.
    let first_sound = f.full.iter().position(|v| db(*v) > SILENCE_DB).unwrap_or(0);
    let last_sound = f.full.iter().rposition(|v| db(*v) > SILENCE_DB).unwrap_or(n - 1);
    let lead_silence_ms = ms(first_sound as f64);
    let tail_silence_ms = (f.duration_ms - ms(last_sound as f64 + 1.0)).max(0.0);

    // Tempo et grille.
    let o = onsets(&f.low);
    let bpm0 = tag_bpm.filter(|b| (60.0..=200.0).contains(b)).or_else(|| crate::djtools::analysis::bpm_from_onsets(&o, f.fps)).unwrap_or(120.0);
    let (bpm, phase, confidence) = fit_grid(&o, f.fps, bpm0);
    let period = 60.0 * f.fps / bpm;
    // Premier temps fort : le premier temps de la grille à partir du début du son
    // (tolérance d'un quart de temps avant).
    let mut first = phase;
    while first + period * 0.25 < first_sound as f64 {
        first += period;
    }
    while first - period >= first_sound as f64 - period * 0.25 {
        first -= period;
    }
    let first_beat_ms = ms(first.max(0.0));

    // Énergie par mesure (4 temps).
    let bar_len = period * 4.0;
    let mut bar_e = Vec::new();
    let mut bar_l = Vec::new();
    let mut t = first;
    while (t + bar_len) as usize <= last_sound + 1 && ((t + bar_len) as usize) <= n {
        let (a, b) = (t as usize, (t + bar_len) as usize);
        let mean = |v: &[f32]| v[a..b].iter().map(|x| *x as f64).sum::<f64>() / (b - a).max(1) as f64;
        bar_e.push(10.0 * mean(&f.full).max(1e-12).log10());
        bar_l.push(10.0 * mean(&f.low).max(1e-12).log10());
        t += bar_len;
    }
    let bars = bar_e.len();
    let sections = sections(&bar_e, &bar_l, first_beat_ms, bar_len * 1000.0 / f.fps);
    let cues = cues(&sections, first_beat_ms);
    Ok(Structure {
        bpm: (bpm * 100.0).round() / 100.0,
        first_beat_ms,
        bars,
        sections,
        cues,
        lead_silence_ms,
        tail_silence_ms,
        duration_ms: f.duration_ms,
        timbre: f.timbre,
        confidence,
    })
}

/// Découpe en blocs de 8 mesures, étiquetés d'après l'énergie (globale et des graves)
/// relative au maximum du morceau, puis fusion des blocs voisins identiques.
fn sections(bar_e: &[f64], bar_l: &[f64], first_ms: f64, bar_ms: f64) -> Vec<Section> {
    let bars = bar_e.len();
    if bars == 0 {
        return Vec::new();
    }
    let block = 8;
    let blocks = bars.div_ceil(block);
    let avg = |v: &[f64], i: usize| {
        let a = i * block;
        let b = ((i + 1) * block).min(bars);
        // Moyenne en puissance.
        10.0 * (v[a..b].iter().map(|d| 10f64.powf(d / 10.0)).sum::<f64>() / (b - a) as f64).log10()
    };
    let e: Vec<f64> = (0..blocks).map(|i| avg(bar_e, i)).collect();
    let l: Vec<f64> = (0..blocks).map(|i| avg(bar_l, i)).collect();
    // Référence : le haut du morceau (80e centile), pour ne pas dépendre d'un seul bloc.
    let pct = |v: &[f64]| {
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.total_cmp(b));
        s[((s.len() - 1) as f64 * 0.8).round() as usize]
    };
    let (e_ref, l_ref) = (pct(&e), pct(&l));
    let is_drop = |i: usize| e[i] >= e_ref - 2.5 && l[i] >= l_ref - 4.0;
    let first_drop = (0..blocks).find(|i| is_drop(*i));
    let last_drop = (0..blocks).rev().find(|i| is_drop(*i));
    let mut kinds: Vec<&str> = Vec::with_capacity(blocks);
    for i in 0..blocks {
        let k = match (first_drop, last_drop) {
            (Some(fd), Some(ld)) => {
                if i < fd {
                    // Bloc juste avant le drop, plus pauvre en graves : la montée.
                    if i + 1 == fd && i > 0 && l[i] < l_ref - 6.0 { "build" } else { "intro" }
                } else if i > ld {
                    "outro"
                } else if is_drop(i) {
                    "drop"
                } else if l[i] < l_ref - 10.0 || e[i] < e_ref - 8.0 {
                    "break"
                } else {
                    "build"
                }
            }
            _ => {
                if i < 2 { "intro" } else if i + 2 >= blocks { "outro" } else { "main" }
            }
        };
        kinds.push(k);
    }
    let mut out: Vec<Section> = Vec::new();
    for (i, k) in kinds.iter().enumerate() {
        let start_bar = i * block;
        let nb = block.min(bars - start_bar);
        match out.last_mut() {
            Some(s) if s.kind == *k => {
                s.bars += nb;
                s.end_ms = first_ms + (s.start_bar + s.bars) as f64 * bar_ms;
            }
            _ => out.push(Section {
                kind: k.to_string(),
                start_ms: first_ms + start_bar as f64 * bar_ms,
                end_ms: first_ms + (start_bar + nb) as f64 * bar_ms,
                start_bar,
                bars: nb,
            }),
        }
    }
    out
}

/// Repères proposés : directs A (début), B (premier drop), C (premier break après le drop),
/// D (outro) ; un repère mémoire au début de chaque section.
fn cues(sections: &[Section], first_ms: f64) -> Vec<Cue> {
    let mut out = vec![Cue { hot: Some(0), name: "start".into(), ms: first_ms }];
    let drop = sections.iter().position(|s| s.kind == "drop");
    if let Some(d) = drop {
        out.push(Cue { hot: Some(1), name: "drop".into(), ms: sections[d].start_ms });
        if let Some(b) = sections[d..].iter().find(|s| s.kind == "break") {
            out.push(Cue { hot: Some(2), name: "break".into(), ms: b.start_ms });
        }
    }
    if let Some(o) = sections.iter().find(|s| s.kind == "outro") {
        out.push(Cue { hot: Some(3), name: "outro".into(), ms: o.start_ms });
    }
    for s in sections {
        out.push(Cue { hot: None, name: s.kind.clone(), ms: s.start_ms });
    }
    out
}

// =====================================================================
// Base de données
// =====================================================================

pub fn get(conn: &Connection, id: i64) -> Result<Option<Structure>, AppError> {
    let raw: Option<String> = conn.query_row("SELECT data FROM structure WHERE track_id = ?1", [id], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|r| serde_json::from_str(&r).ok()))
}

fn save(conn: &Connection, id: i64, size: i64, mtime: i64, s: &Structure) -> Result<(), AppError> {
    let data = serde_json::to_string(s).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    conn.execute(
        "INSERT INTO structure (track_id, size, mtime, data, lead_silence_ms, tail_silence_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, data = excluded.data,
           lead_silence_ms = excluded.lead_silence_ms, tail_silence_ms = excluded.tail_silence_ms, analyzed_at = unixepoch()",
        params![id, size, mtime, data, s.lead_silence_ms.round() as i64, s.tail_silence_ms.round() as i64],
    )?;
    Ok(())
}

/// Structure d'un morceau, analysée sur-le-champ si elle manque ou n'est plus à jour.
pub fn ensure(lib: &Library, conn: &Connection, id: i64) -> Result<Structure, AppError> {
    let row: Option<(i64, i64, Option<f64>, bool)> = conn
        .query_row(
            "SELECT t.size, t.mtime, t.bpm, s.track_id IS NOT NULL AND s.size = t.size AND s.mtime = t.mtime
             FROM tracks t LEFT JOIN structure s ON s.track_id = t.id WHERE t.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((size, mtime, bpm, fresh)) = row else { return Err(AppError::Database(rusqlite::Error::QueryReturnedNoRows)) };
    if fresh {
        if let Some(s) = get(conn, id)? {
            return Ok(s);
        }
    }
    let path = lib.track_path(conn, id)?.ok_or(AppError::Offline)?;
    let s = analyze(&path, bpm).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
    save(conn, id, size, mtime, &s)?;
    Ok(s)
}

/// Morceaux à analyser : sans structure à jour (ou `ids`, même à jour).
fn pending(conn: &Connection, ids: Option<&[i64]>) -> Result<Vec<(i64, i64, String, i64, i64, Option<f64>)>, AppError> {
    let mut st = conn.prepare(
        "SELECT t.id, t.volume_id, t.rel_path, t.size, t.mtime, t.bpm FROM tracks t
         LEFT JOIN structure s ON s.track_id = t.id
         WHERE t.status = 'ok' AND (?1 OR s.track_id IS NULL OR s.size != t.size OR s.mtime != t.mtime)",
    )?;
    let rows: Vec<(i64, i64, String, i64, i64, Option<f64>)> =
        st.query_map([ids.is_some()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<Result<_, _>>()?;
    Ok(match ids {
        Some(ids) => rows.into_iter().filter(|r| ids.contains(&r.0)).collect(),
        None => rows,
    })
}

/// Analyse de structure en tâche de fond (sélection, ou tous les morceaux sans résultat).
pub fn run(lib: &Library, conn: &Connection, ids: Option<&[i64]>, task: Option<&TaskHandle>) -> Result<usize, AppError> {
    let todo = pending(conn, ids)?;
    if let Some(t) = task {
        t.set_total(todo.len() as u64);
    }
    let mounts = lib.mounts();
    let mut done = 0;
    for (i, (id, vol, rel, size, mtime, bpm)) in todo.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        if let Some(m) = mounts.get(vol) {
            match analyze(&volumes::join_rel(m, rel), *bpm) {
                Ok(s) => {
                    save(conn, *id, *size, *mtime, &s)?;
                    done += 1;
                }
                Err(e) => log::info!("Structure impossible ({rel}) : {e}"),
            }
        }
        if let Some(t) = task {
            t.progress(i as u64 + 1);
            if i % 20 == 19 {
                lib.notify_changed(false);
            }
        }
    }
    lib.notify_changed(true);
    Ok(done)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Morceau de synthèse : 1 s de silence, intro (grosse caisse seule) 16 mesures,
    /// drop (grosse caisse + basse + nappe) 16 mesures, break (nappe seule) 8 mesures,
    /// drop 16 mesures, outro (grosse caisse) 8 mesures, 2 s de silence. 128 BPM.
    pub fn club_track() -> Vec<i16> {
        let rate = 44_100.0;
        let bpm = 128.0;
        let beat = 60.0 / bpm;
        let plan = [("intro", 16), ("drop", 16), ("break", 8), ("drop", 16), ("outro", 8)];
        let total_bars: usize = plan.iter().map(|p| p.1).sum();
        let music = total_bars as f64 * 4.0 * beat;
        let n = ((1.0 + music + 2.0) * rate) as usize;
        let mut s = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f64 / rate - 1.0;
            let mut v = 0.0;
            if t >= 0.0 && t < music {
                let bar = (t / (4.0 * beat)) as usize;
                let mut acc = 0;
                let mut kind = "outro";
                for (k, b) in plan {
                    if bar < acc + b {
                        kind = k;
                        break;
                    }
                    acc += b;
                }
                let tb = t % beat;
                let kick = 0.6 * (-tb * 18.0).exp() * (2.0 * std::f64::consts::PI * (50.0 + 90.0 * (-tb * 35.0).exp()) * tb).sin();
                let bass = 0.25 * (2.0 * std::f64::consts::PI * 55.0 * t).sin() * (if (t / (beat / 2.0)) as usize % 2 == 1 { 1.0 } else { 0.2 });
                let pad = 0.08 * ((2.0 * std::f64::consts::PI * 440.0 * t).sin() + (2.0 * std::f64::consts::PI * 660.0 * t).sin() + 0.5 * (2.0 * std::f64::consts::PI * 3300.0 * t).sin());
                v = match kind {
                    "intro" | "outro" => kick,
                    "drop" => kick + bass + pad,
                    _ => pad,
                };
            }
            let x = (v * 20000.0).clamp(-32767.0, 32767.0) as i16;
            s.push(x);
            s.push(x);
        }
        s
    }

    #[test]
    fn grille_sections_reperes_et_silences() {
        let dir = tempfile::tempdir().unwrap();
        let p = crate::testdata::write(dir.path(), "club.wav", &crate::testdata::wav_from_samples(&club_track()));
        let st = analyze(&p, None).unwrap();
        assert!((st.bpm - 128.0).abs() < 0.1, "{}", st.bpm);
        assert!((st.first_beat_ms - 1000.0).abs() < 15.0, "{}", st.first_beat_ms);
        assert!((st.lead_silence_ms - 1000.0).abs() < 20.0, "{}", st.lead_silence_ms);
        // La dernière grosse caisse s'éteint ~0,2 s avant la fin de la musique.
        assert!((1950.0..2350.0).contains(&st.tail_silence_ms), "{}", st.tail_silence_ms);
        assert!(st.confidence > 0.3, "{}", st.confidence);
        let kinds: Vec<&str> = st.sections.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["intro", "drop", "break", "drop", "outro"], "{:?}", st.sections);
        let bar = st.bar_ms();
        let drop = st.cues.iter().find(|c| c.hot == Some(1)).unwrap();
        assert!((drop.ms - (1000.0 + 16.0 * bar)).abs() < 20.0, "{}", drop.ms);
        let brk = st.cues.iter().find(|c| c.hot == Some(2)).unwrap();
        assert!((brk.ms - (1000.0 + 32.0 * bar)).abs() < 20.0);
        // Le tempo du tag sert de point de départ.
        let st2 = analyze(&p, Some(128.0)).unwrap();
        assert!((st2.bpm - 128.0).abs() < 0.05);
    }

    /// Diagnostic (lecture seule) : DJLM_DIAG_DIR=dossier, DJLM_DIAG_STEP=un fichier sur N.
    #[test]
    #[ignore]
    fn structure_reelle() {
        let Ok(dir) = std::env::var("DJLM_DIAG_DIR") else { return };
        let step: usize = std::env::var("DJLM_DIAG_STEP").ok().and_then(|s| s.parse().ok()).unwrap_or(300);
        for (i, e) in std::fs::read_dir(dir).unwrap().flatten().enumerate() {
            let p = e.path();
            if i % step != 0 || !crate::tags::is_audio_path(&p) {
                continue;
            }
            let tag = crate::tags::read::read_file(&p, &[]).ok().and_then(|f| f.tags.bpm);
            let t0 = std::time::Instant::now();
            match analyze(&p, tag) {
                Ok(s) => println!(
                    "STRUCT {:?} tag {:?} -> {} BPM, 1er temps {:.0} ms, nette {:.2}, silences {:.0}/{:.0} ms, {:.1} s : {}",
                    p.file_name().unwrap(), tag, s.bpm, s.first_beat_ms, s.confidence, s.lead_silence_ms, s.tail_silence_ms, t0.elapsed().as_secs_f64(),
                    s.sections.iter().map(|x| format!("{}:{}", x.kind, x.bars)).collect::<Vec<_>>().join(" ")
                ),
                Err(e) => println!("STRUCT {:?} erreur {e}", p.file_name().unwrap()),
            }
        }
    }
}
