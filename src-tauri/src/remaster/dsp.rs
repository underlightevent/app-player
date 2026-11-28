//! Chaîne de traitement du son (phase 7) : passe-haut, égaliseur 5 bandes, égalisation
//! mono / stéréo, compresseur, gain, limiteur à anticipation. Travaille sur des échantillons
//! flottants entrelacés (1 ou 2 canaux). Aucun fichier n'est touché ici.

use std::collections::VecDeque;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

// =====================================================================
// Réglages
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BandKind {
    LowShelf,
    Peak,
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Band {
    pub kind: BandKind,
    pub freq: f64,
    pub gain_db: f64,
    pub q: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Passe-haut (Hz) ; 0 = désactivé.
    pub highpass_hz: f64,
    pub eq_on: bool,
    pub bands: Vec<Band>,
    /// Largeur stéréo : 0 = mono, 1 = d'origine, jusqu'à 1,5.
    pub width: f64,
    /// Graves en mono sous cette fréquence (Hz) ; 0 = désactivé.
    pub mono_bass_hz: f64,
    pub comp_on: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_db: f64,
    /// Volume cible (LUFS) ; `None` = pas de normalisation.
    pub target_lufs: Option<f64>,
    pub limiter_on: bool,
    /// Plafond de crête (dBFS).
    pub ceiling_db: f64,
}

/// Les 5 bandes de l'égaliseur, à plat.
pub fn default_bands() -> Vec<Band> {
    vec![
        Band { kind: BandKind::LowShelf, freq: 90.0, gain_db: 0.0, q: 0.7 },
        Band { kind: BandKind::Peak, freq: 250.0, gain_db: 0.0, q: 1.0 },
        Band { kind: BandKind::Peak, freq: 1000.0, gain_db: 0.0, q: 1.0 },
        Band { kind: BandKind::Peak, freq: 3500.0, gain_db: 0.0, q: 1.0 },
        Band { kind: BandKind::HighShelf, freq: 10000.0, gain_db: 0.0, q: 0.7 },
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            highpass_hz: 25.0,
            eq_on: true,
            bands: default_bands(),
            width: 1.0,
            mono_bass_hz: 0.0,
            comp_on: false,
            threshold_db: -18.0,
            ratio: 2.0,
            attack_ms: 20.0,
            release_ms: 200.0,
            makeup_db: 0.0,
            target_lufs: None,
            limiter_on: true,
            ceiling_db: -1.0,
        }
    }
}

impl Settings {
    /// Valeurs ramenées dans des bornes sûres (réglages venus de l'interface).
    pub fn clamped(&self) -> Settings {
        let mut s = self.clone();
        s.highpass_hz = s.highpass_hz.clamp(0.0, 200.0);
        s.bands.truncate(8);
        for b in s.bands.iter_mut() {
            b.freq = b.freq.clamp(20.0, 20_000.0);
            b.gain_db = b.gain_db.clamp(-12.0, 12.0);
            b.q = b.q.clamp(0.2, 8.0);
        }
        s.width = s.width.clamp(0.0, 1.5);
        s.mono_bass_hz = s.mono_bass_hz.clamp(0.0, 300.0);
        s.threshold_db = s.threshold_db.clamp(-60.0, 0.0);
        s.ratio = s.ratio.clamp(1.0, 20.0);
        s.attack_ms = s.attack_ms.clamp(0.1, 200.0);
        s.release_ms = s.release_ms.clamp(10.0, 2000.0);
        s.makeup_db = s.makeup_db.clamp(-12.0, 24.0);
        s.target_lufs = s.target_lufs.map(|t| t.clamp(-30.0, -4.0));
        s.ceiling_db = s.ceiling_db.clamp(-6.0, 0.0);
        s
    }
}

// =====================================================================
// Filtres (biquads, formules de R. Bristow-Johnson)
// =====================================================================

#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Biquad {
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    pub fn highpass(rate: f64, f: f64, q: f64) -> Biquad {
        let w = 2.0 * PI * f / rate;
        let (c, s) = (w.cos(), w.sin());
        let alpha = s / (2.0 * q);
        Biquad::new((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn band(rate: f64, b: &Band) -> Biquad {
        let a = 10f64.powf(b.gain_db / 40.0);
        let w = 2.0 * PI * b.freq.min(rate * 0.45) / rate;
        let (c, s) = (w.cos(), w.sin());
        match b.kind {
            BandKind::Peak => {
                let alpha = s / (2.0 * b.q);
                Biquad::new(1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a)
            }
            BandKind::LowShelf | BandKind::HighShelf => {
                // Pente S = 1.
                let alpha = s / 2.0 * ((a + 1.0 / a) * (1.0 / 1.0 - 1.0) + 2.0).sqrt();
                let sq = 2.0 * a.sqrt() * alpha;
                if b.kind == BandKind::LowShelf {
                    Biquad::new(
                        a * ((a + 1.0) - (a - 1.0) * c + sq),
                        2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                        a * ((a + 1.0) - (a - 1.0) * c - sq),
                        (a + 1.0) + (a - 1.0) * c + sq,
                        -2.0 * ((a - 1.0) + (a + 1.0) * c),
                        (a + 1.0) + (a - 1.0) * c - sq,
                    )
                } else {
                    Biquad::new(
                        a * ((a + 1.0) + (a - 1.0) * c + sq),
                        -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                        a * ((a + 1.0) + (a - 1.0) * c - sq),
                        (a + 1.0) - (a - 1.0) * c + sq,
                        2.0 * ((a - 1.0) - (a + 1.0) * c),
                        (a + 1.0) - (a - 1.0) * c - sq,
                    )
                }
            }
        }
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        // Forme directe transposée II.
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

// =====================================================================
// Processeur
// =====================================================================

pub struct Processor {
    channels: usize,
    s: Settings,
    hp: Vec<Biquad>,
    eq: Vec<Vec<Biquad>>,
    side_hp: Vec<Biquad>,
    width: f64,
    // Compresseur (détecteur stéréo lié, en dB)
    comp_env_db: f64,
    att: f64,
    rel: f64,
    // Gain total (compensation + volume cible)
    gain: f64,
    // Limiteur à anticipation
    ceiling: f64,
    look: usize,
    delay: VecDeque<[f64; 2]>,
    req: VecDeque<(usize, f64)>,
    n: usize,
    lim_g: f64,
    lim_att: f64,
    lim_rel: f64,
}

impl Processor {
    /// `extra_gain_db` : gain ajouté avant le limiteur (atteinte du volume cible).
    pub fn new(settings: &Settings, rate: u32, channels: u16, extra_gain_db: f64) -> Processor {
        let s = settings.clamped();
        let rate = rate.max(8000) as f64;
        let channels = (channels as usize).clamp(1, 2);
        let mk_hp = || Biquad::highpass(rate, s.highpass_hz.max(1.0), std::f64::consts::FRAC_1_SQRT_2);
        let hp = if s.highpass_hz > 0.0 { vec![mk_hp(), mk_hp()] } else { vec![] };
        let eq = if s.eq_on {
            (0..2).map(|_| s.bands.iter().filter(|b| b.gain_db.abs() > 0.01).map(|b| Biquad::band(rate, b)).collect()).collect()
        } else {
            vec![vec![], vec![]]
        };
        // Deux passages (24 dB/octave) : les graves du canal « différence » disparaissent vraiment.
        let side_hp = if channels == 2 && s.mono_bass_hz > 0.0 {
            vec![Biquad::highpass(rate, s.mono_bass_hz, std::f64::consts::FRAC_1_SQRT_2); 2]
        } else {
            vec![]
        };
        let tc = |ms: f64| (-1.0 / (ms / 1000.0 * rate)).exp();
        let look = ((rate * 0.005) as usize).max(1);
        Processor {
            channels,
            width: s.width,
            hp,
            eq,
            side_hp,
            comp_env_db: 0.0,
            att: tc(s.attack_ms),
            rel: tc(s.release_ms),
            gain: db_to_lin(s.makeup_db * s.comp_on as i32 as f64 + extra_gain_db),
            ceiling: db_to_lin(s.ceiling_db),
            look,
            delay: VecDeque::with_capacity(look + 1),
            req: VecDeque::new(),
            n: 0,
            lim_g: 1.0,
            lim_att: tc(1.0),
            lim_rel: tc(80.0),
            s,
        }
    }

    /// Gain du compresseur (dB, négatif) pour un niveau d'entrée `x_db` (genou doux de 6 dB).
    fn comp_gain_db(&self, x_db: f64) -> f64 {
        let (t, r, w) = (self.s.threshold_db, self.s.ratio, 6.0);
        if x_db < t - w / 2.0 {
            0.0
        } else if x_db > t + w / 2.0 {
            (t + (x_db - t) / r) - x_db
        } else {
            (1.0 / r - 1.0) * (x_db - t + w / 2.0).powi(2) / (2.0 * w)
        }
    }

    /// Une trame (gauche, droite) avant le limiteur.
    #[inline]
    fn pre(&mut self, mut l: f64, mut r: f64) -> (f64, f64) {
        if !self.hp.is_empty() {
            l = self.hp[0].process(l);
            r = self.hp[1].process(r);
        }
        for f in self.eq[0].iter_mut() {
            l = f.process(l);
        }
        for f in self.eq[1].iter_mut() {
            r = f.process(r);
        }
        if self.channels == 2 {
            let m = (l + r) * 0.5;
            let mut side = (l - r) * 0.5 * self.width;
            for f in self.side_hp.iter_mut() {
                side = f.process(side);
            }
            l = m + side;
            r = m - side;
        }
        if self.s.comp_on {
            let peak = l.abs().max(r.abs()).max(1e-9);
            let x_db = 20.0 * peak.log10();
            let target = self.comp_gain_db(x_db);
            // Attaque quand le gain baisse, relâchement quand il remonte.
            let coef = if target < self.comp_env_db { self.att } else { self.rel };
            self.comp_env_db = target + coef * (self.comp_env_db - target);
            let g = db_to_lin(self.comp_env_db);
            l *= g;
            r *= g;
        }
        (l * self.gain, r * self.gain)
    }

    /// Limiteur : gain le plus faible exigé par les échantillons à venir (fenêtre glissante),
    /// lissé, puis plafond strict en dernier recours. Renvoie la trame retardée.
    #[inline]
    fn limit(&mut self, l: f64, r: f64) -> Option<[f64; 2]> {
        if !self.s.limiter_on {
            return Some([l, r]);
        }
        let peak = l.abs().max(r.abs());
        let need = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };
        // Minimum glissant (file monotone).
        while self.req.back().map(|b| b.1 >= need).unwrap_or(false) {
            self.req.pop_back();
        }
        self.req.push_back((self.n, need));
        while self.req.front().map(|f| f.0 + self.look <= self.n).unwrap_or(false) {
            self.req.pop_front();
        }
        self.n += 1;
        self.delay.push_back([l, r]);
        if self.delay.len() <= self.look {
            return None;
        }
        let target = self.req.front().map(|f| f.1).unwrap_or(1.0);
        let coef = if target < self.lim_g { self.lim_att } else { self.lim_rel };
        self.lim_g = target + coef * (self.lim_g - target);
        let [dl, dr] = self.delay.pop_front().unwrap_or([0.0, 0.0]);
        let c = self.ceiling;
        Some([(dl * self.lim_g).clamp(-c, c), (dr * self.lim_g).clamp(-c, c)])
    }

    /// Traite un bloc entrelacé ; la sortie peut être plus courte de quelques millisecondes
    /// (retard du limiteur), rattrapé par `flush` en fin de morceau.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        out.clear();
        let ch = self.channels;
        for f in input.chunks(ch) {
            let (l, r) = if ch == 2 { (f[0] as f64, *f.get(1).unwrap_or(&f[0]) as f64) } else { (f[0] as f64, f[0] as f64) };
            let (l, r) = self.pre(l, r);
            if let Some([a, b]) = self.limit(l, r) {
                out.push(a as f32);
                if ch == 2 {
                    out.push(b as f32);
                }
            }
        }
    }

    /// Fin du morceau : vide le retard du limiteur.
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        out.clear();
        if !self.s.limiter_on {
            return;
        }
        for _ in 0..self.look {
            if let Some([a, b]) = self.limit(0.0, 0.0) {
                out.push(a as f32);
                if self.channels == 2 {
                    out.push(b as f32);
                }
            }
        }
    }

    /// Retard introduit par le limiteur, en trames.
    pub fn latency(&self) -> usize {
        if self.s.limiter_on {
            self.look
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    fn sine(f: f64, amp: f64, secs: f64, stereo: bool) -> Vec<f32> {
        let n = (RATE as f64 * secs) as usize;
        let mut v = Vec::with_capacity(n * 2);
        for i in 0..n {
            let x = (amp * (2.0 * PI * f * i as f64 / RATE as f64).sin()) as f32;
            v.push(x);
            if stereo {
                v.push(x);
            }
        }
        v
    }

    fn rms(v: &[f32]) -> f64 {
        (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt()
    }

    fn run(s: &Settings, input: &[f32], ch: u16) -> Vec<f32> {
        let mut p = Processor::new(s, RATE, ch, 0.0);
        let mut out = Vec::new();
        let mut all = Vec::new();
        for block in input.chunks(4096) {
            p.process(block, &mut out);
            all.extend_from_slice(&out);
        }
        p.flush(&mut out);
        all.extend_from_slice(&out);
        all
    }

    fn flat() -> Settings {
        Settings { highpass_hz: 0.0, limiter_on: false, ..Default::default() }
    }

    #[test]
    fn neutre_ne_change_rien() {
        let x = sine(1000.0, 0.5, 1.0, true);
        let y = run(&flat(), &x, 2);
        assert_eq!(x.len(), y.len());
        assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() < 1e-5));
    }

    #[test]
    fn egaliseur_monte_la_bande_visee() {
        let mut s = flat();
        s.bands[2].gain_db = 6.0; // 1 kHz
        let at = |f: f64| {
            let x = sine(f, 0.1, 1.0, false);
            let y = run(&s, &x, 1);
            20.0 * (rms(&y[22050..]) / rms(&x[22050..])).log10()
        };
        assert!((at(1000.0) - 6.0).abs() < 0.3, "{}", at(1000.0));
        assert!(at(100.0).abs() < 0.5 && at(10_000.0).abs() < 0.5);
        let mut lo = flat();
        lo.bands[0].gain_db = 4.0; // plateau grave
        let x = sine(40.0, 0.1, 1.0, false);
        let y = run(&lo, &x, 1);
        assert!((20.0 * (rms(&y[22050..]) / rms(&x[22050..])).log10() - 4.0).abs() < 0.5);
    }

    #[test]
    fn passe_haut_coupe_les_infra_graves() {
        let s = Settings { highpass_hz: 30.0, limiter_on: false, ..Default::default() };
        let x = sine(10.0, 0.3, 2.0, false);
        let y = run(&s, &x, 1);
        assert!(rms(&y[44100..]) < rms(&x[44100..]) * 0.15);
    }

    #[test]
    fn graves_en_mono_et_largeur() {
        // Graves en opposition de phase (stéréo « large ») + aigus décalés.
        let n = RATE as usize;
        let mut x = Vec::new();
        for i in 0..n {
            let t = i as f64 / RATE as f64;
            let bass = 0.3 * (2.0 * PI * 50.0 * t).sin();
            let hi = 0.2 * (2.0 * PI * 3000.0 * t).sin();
            x.push((bass + hi) as f32);
            x.push((-bass + 0.5 * hi) as f32);
        }
        let s = Settings { mono_bass_hz: 120.0, ..flat() };
        let y = run(&s, &x, 2);
        // Côté (L − R) filtré sous 120 Hz : la différence ne contient presque plus de 50 Hz.
        let side50: Vec<f32> = y.chunks(2).skip(22050).map(|f| (f[0] - f[1]) / 2.0).collect();
        let lowpass = |v: &[f32]| {
            let mut f = Biquad::band(RATE as f64, &Band { kind: BandKind::LowShelf, freq: 200.0, gain_db: -40.0, q: 0.7 });
            v.iter().map(|x| *x as f64 - f.process(*x as f64)).collect::<Vec<f64>>()
        };
        let low_part = lowpass(&side50);
        let low_rms = (low_part.iter().map(|x| x * x).sum::<f64>() / low_part.len() as f64).sqrt();
        assert!(low_rms < 0.03, "graves restés en stéréo : {low_rms}");
        // Largeur 0 = mono strict.
        let m = run(&Settings { width: 0.0, ..flat() }, &x, 2);
        assert!(m.chunks(2).all(|f| (f[0] - f[1]).abs() < 1e-6));
    }

    #[test]
    fn compresseur_reduit_la_dynamique() {
        let s = Settings { comp_on: true, threshold_db: -20.0, ratio: 4.0, attack_ms: 5.0, release_ms: 100.0, ..flat() };
        let loud = run(&s, &sine(1000.0, 0.9, 1.0, false), 1);
        let quiet = run(&s, &sine(1000.0, 0.05, 1.0, false), 1);
        let g_loud = 20.0 * (rms(&loud[22050..]) / (0.9 / 2f64.sqrt())).log10();
        let g_quiet = 20.0 * (rms(&quiet[22050..]) / (0.05 / 2f64.sqrt())).log10();
        assert!(g_loud < -8.0, "fort : {g_loud}");
        assert!(g_quiet.abs() < 0.5, "faible intact : {g_quiet}");
    }

    #[test]
    fn limiteur_ne_depasse_jamais_le_plafond() {
        let s = Settings { ceiling_db: -1.0, ..flat() };
        let s = Settings { limiter_on: true, ..s };
        let mut x = sine(100.0, 0.5, 1.0, true);
        for (i, v) in x.iter_mut().enumerate() {
            if i % 9000 < 40 {
                *v *= 3.5; // crêtes brutales
            }
        }
        let mut p = Processor::new(&s, RATE, 2, 6.0);
        let mut out = Vec::new();
        let mut all = Vec::new();
        for b in x.chunks(1000) {
            p.process(b, &mut out);
            all.extend_from_slice(&out);
        }
        p.flush(&mut out);
        all.extend_from_slice(&out);
        let ceil = 10f32.powf(-1.0 / 20.0);
        assert!(all.iter().all(|v| v.abs() <= ceil + 1e-6));
        assert_eq!(all.len(), x.len(), "retard rattrapé");
    }
}
