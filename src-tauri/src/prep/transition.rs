//! Essai d'un enchaînement (phase 8) : la fin du morceau A, puis le début du morceau B
//! mélangé sur 16 ou 32 mesures, B accéléré ou ralenti pour suivre le tempo de A (comme une
//! platine sans « master tempo » : la hauteur suit), calé sur les grilles des deux morceaux.
//! Écoute seulement : rien n'est enregistré.

use std::collections::VecDeque;
use std::num::NonZero;
use std::path::PathBuf;
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};
use serde::Serialize;

use super::structure::Structure;
use crate::audio::decode::Decoder;

/// Écart de tempo maximal accepté (±8 %, comme une platine).
pub const MAX_PITCH: f64 = 0.08;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(skip)]
    pub a_path: PathBuf,
    #[serde(skip)]
    pub b_path: PathBuf,
    /// Début de l'écoute dans A (ms) : quelques mesures avant le mélange.
    pub a_from_ms: f64,
    /// Début du mélange dans A (ms, sur un premier temps de mesure).
    pub mix_at_ms: f64,
    /// Durée du mélange (ms, au tempo de A).
    pub mix_ms: f64,
    /// Début de B (ms, son premier temps fort).
    pub b_from_ms: f64,
    /// Vitesse de lecture de B (1,02 = 2 % plus vite).
    pub b_speed: f64,
    /// Durée totale de l'écoute (ms).
    pub total_ms: f64,
    pub bars: usize,
}

/// Calcule l'enchaînement. `bars` : longueur du mélange (16 ou 32 mesures).
pub fn plan(a_path: PathBuf, a: &Structure, b_path: PathBuf, b: &Structure, bars: usize) -> Result<Plan, String> {
    // Tempo de B ramené près de celui de A (demi / double tempo compris).
    let mut bpm_b = b.bpm;
    for f in [2.0, 0.5] {
        if ((b.bpm * f) / a.bpm - 1.0).abs() < (bpm_b / a.bpm - 1.0).abs() {
            bpm_b = b.bpm * f;
        }
    }
    let speed = a.bpm / bpm_b;
    if (speed - 1.0).abs() > MAX_PITCH {
        return Err("tempo".into());
    }
    let bar = a.bar_ms();
    let mix_ms = bars as f64 * bar;
    // Fin de la musique de A, puis point de mélange : début de l'outro s'il laisse assez de
    // place, sinon la dernière mesure qui permet un mélange complet.
    let music_end = a.duration_ms - a.tail_silence_ms;
    let last_fit = ((music_end - mix_ms - a.first_beat_ms) / bar).floor().max(0.0);
    let mut mix_bar = last_fit;
    if let Some(o) = a.sections.iter().find(|s| s.kind == "outro") {
        if o.start_ms + mix_ms <= music_end + bar {
            mix_bar = o.start_bar as f64;
        }
    }
    let mix_at_ms = a.first_beat_ms + mix_bar * bar;
    let a_from_ms = (mix_at_ms - 8.0 * bar).max(a.first_beat_ms.min(mix_at_ms));
    // Après le mélange, encore 16 mesures de B (ou moins si B est court).
    let b_left = (b.duration_ms - b.tail_silence_ms - b.first_beat_ms) / speed;
    let after = (16.0 * bar).min((b_left - mix_ms).max(0.0));
    Ok(Plan {
        a_path,
        b_path,
        a_from_ms,
        mix_at_ms,
        mix_ms,
        b_from_ms: b.first_beat_ms,
        b_speed: speed,
        total_ms: (mix_at_ms - a_from_ms) + mix_ms + after,
        bars,
    })
}

/// Source audio de l'enchaînement (stéréo, à la fréquence de A).
pub struct MixSource {
    plan: Plan,
    a: Decoder,
    b: Decoder,
    rate: u32,
    a_buf: VecDeque<[f32; 2]>,
    a_done: bool,
    /// Trames de B décodées (stéréo), position fractionnaire de lecture.
    b_buf: VecDeque<[f32; 2]>,
    b_pos: f64,
    b_step: f64,
    b_done: bool,
    /// Trame de sortie courante.
    k: u64,
    mix_k: u64,
    mix_len: u64,
    total: u64,
    pending: Option<f32>,
    block: Vec<f32>,
}

fn stereo(block: &[f32], ch: usize, out: &mut VecDeque<[f32; 2]>) {
    for f in block.chunks(ch.max(1)) {
        let l = f[0];
        let r = if ch >= 2 { f[1] } else { l };
        out.push_back([l, r]);
    }
}

impl MixSource {
    pub fn new(plan: Plan) -> Result<MixSource, String> {
        let a = Decoder::open(&plan.a_path)?;
        let b = Decoder::open(&plan.b_path)?;
        let rate = a.sample_rate;
        let r = rate as f64 / 1000.0;
        let mut s = MixSource {
            mix_k: ((plan.mix_at_ms - plan.a_from_ms) * r).round() as u64,
            mix_len: (plan.mix_ms * r).round().max(1.0) as u64,
            total: (plan.total_ms * r).round() as u64,
            b_step: plan.b_speed * b.sample_rate as f64 / rate as f64,
            plan,
            a,
            b,
            rate,
            a_buf: VecDeque::new(),
            a_done: false,
            b_buf: VecDeque::new(),
            b_pos: 0.0,
            b_done: false,
            k: 0,
            pending: None,
            block: Vec::new(),
        };
        s.seek_to(0)?;
        Ok(s)
    }

    /// Place les deux morceaux à la trame de sortie `k`.
    fn seek_to(&mut self, k: u64) -> Result<(), String> {
        let r = self.rate as f64 / 1000.0;
        self.k = k;
        self.a_buf.clear();
        self.b_buf.clear();
        self.a_done = false;
        self.b_done = false;
        self.pending = None;
        self.a.seek_exact(self.plan.a_from_ms + k as f64 / r)?;
        // Position de B (en trames de B) au moment `k`.
        let b_ms = self.plan.b_from_ms + (k.saturating_sub(self.mix_k) as f64 / r) * self.plan.b_speed;
        self.b.seek_exact(b_ms)?;
        self.b_pos = 0.0;
        Ok(())
    }

    fn next_a(&mut self) -> [f32; 2] {
        while self.a_buf.is_empty() && !self.a_done {
            match self.a.next_block(&mut self.block) {
                Ok(true) => stereo(&self.block, self.a.channels as usize, &mut self.a_buf),
                _ => self.a_done = true,
            }
        }
        self.a_buf.pop_front().unwrap_or([0.0, 0.0])
    }

    /// Trame de B rééchantillonnée (interpolation linéaire).
    fn next_b(&mut self) -> [f32; 2] {
        while self.b_buf.len() < self.b_pos as usize + 2 && !self.b_done {
            match self.b.next_block(&mut self.block) {
                Ok(true) => stereo(&self.block, self.b.channels as usize, &mut self.b_buf),
                _ => self.b_done = true,
            }
        }
        let i = self.b_pos as usize;
        let f = (self.b_pos - i as f64) as f32;
        let x0 = self.b_buf.get(i).copied().unwrap_or([0.0, 0.0]);
        let x1 = self.b_buf.get(i + 1).copied().unwrap_or(x0);
        self.b_pos += self.b_step;
        // Libère ce qui est déjà joué.
        let drop = (self.b_pos as usize).min(self.b_buf.len());
        if drop > 1024 {
            self.b_buf.drain(..drop);
            self.b_pos -= drop as f64;
        }
        [x0[0] + (x1[0] - x0[0]) * f, x0[1] + (x1[1] - x0[1]) * f]
    }

    fn frame(&mut self) -> Option<[f32; 2]> {
        if self.k >= self.total {
            return None;
        }
        let k = self.k;
        self.k += 1;
        let half_pi = std::f32::consts::FRAC_PI_2;
        let in_mix = k >= self.mix_k;
        let x = if in_mix { ((k - self.mix_k) as f32 / self.mix_len as f32).min(1.0) } else { 0.0 };
        let ga = if in_mix { (x * half_pi).cos() } else { 1.0 };
        let a = if ga > 0.0 { self.next_a() } else { [0.0, 0.0] };
        let (gb, b) = if in_mix { ((x * half_pi).sin(), self.next_b()) } else { (0.0, [0.0, 0.0]) };
        // Petit fondu sur la dernière demi-seconde.
        let end = ((self.total - k) as f32 / (self.rate as f32 * 0.5)).min(1.0);
        Some([(a[0] * ga + b[0] * gb) * end, (a[1] * ga + b[1] * gb) * end])
    }
}

impl Iterator for MixSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.pending.take() {
            return Some(r);
        }
        let [l, r] = self.frame()?;
        self.pending = Some(r);
        Some(l)
    }
}

impl Source for MixSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.rate).unwrap_or(NonZero::<u32>::MIN)
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_millis(self.plan.total_ms as u64))
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        let k = (pos.as_secs_f64() * self.rate as f64) as u64;
        self.seek_to(k.min(self.total)).map_err(|_| SeekError::NotSupported { underlying_source: "enchaînement" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enchainement_cale_sur_les_temps() {
        let dir = tempfile::tempdir().unwrap();
        let samples = crate::prep::structure::tests::club_track();
        let a = crate::testdata::write(dir.path(), "a.wav", &crate::testdata::wav_from_samples(&samples));
        let b = crate::testdata::write(dir.path(), "b.wav", &crate::testdata::wav_from_samples(&samples));
        let sa = crate::prep::structure::analyze(&a, Some(128.0)).unwrap();
        let mut sb = sa.clone();
        // B annoncé à 126 BPM : il doit être accéléré de 128 / 126.
        sb.bpm = 126.0;
        let p = plan(a.clone(), &sa, b.clone(), &sb, 16).unwrap();
        assert!((p.b_speed - 128.0 / 126.0).abs() < 1e-9);
        // Le mélange commence sur un premier temps de mesure et tient avant la fin de A.
        let bar = sa.bar_ms();
        let mix_bar = (p.mix_at_ms - sa.first_beat_ms) / bar;
        assert!((mix_bar - mix_bar.round()).abs() < 1e-6 && (46.0..=56.0).contains(&mix_bar), "{mix_bar}");
        // La source produit la durée annoncée, sans saturer.
        let src = MixSource::new(p.clone()).unwrap();
        let out: Vec<f32> = src.collect();
        assert_eq!(out.len() as f64, (p.total_ms * 44.1).round() * 2.0);
        assert!(out.iter().all(|x| x.abs() <= 1.0));
        // Tempo trop éloigné : refusé.
        sb.bpm = 100.0;
        assert!(plan(a, &sa, b, &sb, 16).is_err());
    }
}
