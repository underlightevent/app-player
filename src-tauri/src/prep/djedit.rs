//! DJ edit (phase 8) : intro et / ou outro allongées en répétant des mesures, calées sur la
//! grille du morceau. Le résultat est un nouveau WAV 24 bits « … (DJ Edit).wav » ; l'original
//! n'est jamais modifié.
//!
//! Le morceau est décrit comme une suite de plages de l'original (en trames) :
//! [début → 1er temps] [boucle d'intro × N] [1er temps → fin de la musique gardée]
//! [boucle d'outro × N] [suite → fin]. Aux raccords qui ne se suivent pas dans l'original, un
//! fondu enchaîné très court (≈ 6 ms, à puissance constante) évite tout clic.

use serde::{Deserialize, Serialize};

use super::structure::Structure;
use crate::audio::decode::Decoder;
use crate::remaster::engine::Wav24;

/// Longueur du fondu aux raccords (trames).
const XFADE: usize = 256;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    /// Mesures ajoutées avant l'intro (0 = rien).
    pub intro_bars: u32,
    /// Mesures ajoutées après l'outro (0 = rien).
    pub outro_bars: u32,
    /// Longueur de la boucle répétée (4, 8 ou 16 mesures).
    pub loop_bars: u32,
}

/// Plages (début, fin) de l'original, en trames, à mettre bout à bout.
pub fn ranges(st: &Structure, rate: u32, total: u64, plan: &EditPlan) -> Result<Vec<(u64, u64)>, String> {
    let r = rate as f64 / 1000.0;
    let bar = st.bar_ms() * r;
    let lb = plan.loop_bars.clamp(1, 32) as f64;
    let first = (st.first_beat_ms * r).round() as u64;
    let lp = (lb * bar).round() as u64;
    if plan.intro_bars % plan.loop_bars.max(1) != 0 || plan.outro_bars % plan.loop_bars.max(1) != 0 {
        return Err("boucle".into());
    }
    // Fin de la dernière mesure complète avant la fin de la musique.
    let music_end = ((st.duration_ms - st.tail_silence_ms) * r) as u64;
    let full_bars = (((music_end.saturating_sub(first)) as f64) / bar).floor();
    let end = first + (full_bars * bar).round() as u64;
    if first + lp > end || end < lp || end - lp < first {
        return Err("court".into());
    }
    let mut out = Vec::new();
    let mut push = |a: u64, b: u64| {
        if b > a {
            out.push((a, b));
        }
    };
    push(0, first);
    for _ in 0..(plan.intro_bars / plan.loop_bars.max(1)) {
        push(first, first + lp);
    }
    push(first, end);
    for _ in 0..(plan.outro_bars / plan.loop_bars.max(1)) {
        push(end - lp, end);
    }
    push(end, total.max(end));
    Ok(out)
}

/// Nombre exact de trames du fichier.
pub fn count_frames(dec: &mut Decoder) -> Result<u64, String> {
    let ch = dec.channels.max(1) as u64;
    let mut buf = Vec::new();
    let mut n = 0;
    while dec.next_block(&mut buf)? {
        n += buf.len() as u64 / ch;
    }
    Ok(n)
}

/// Écrit le DJ edit de `src` dans `dst` (WAV 24 bits). Renvoie la durée obtenue (ms).
pub fn render(src: &std::path::Path, dst: &std::path::Path, st: &Structure, plan: &EditPlan) -> Result<f64, String> {
    let mut dec = Decoder::open(src)?;
    let (rate, ch) = (dec.sample_rate, dec.channels.max(1) as usize);
    let total = count_frames(&mut dec)?;
    let plan_r = ranges(st, rate, total, plan)?;
    let mut w = Wav24::create(dst, rate, ch as u16).map_err(|e| e.to_string())?;
    let ms = |f: u64| f as f64 * 1000.0 / rate as f64;
    // Suite de la plage précédente (pour le fondu enchaîné), si le raccord est discontinu.
    let mut tail: Option<Vec<f32>> = None;
    let mut written = 0u64;
    let mut buf = Vec::new();
    for (i, (a, b)) in plan_r.iter().copied().enumerate() {
        let next_start = plan_r.get(i + 1).map(|r| r.0);
        let jump = next_start.map(|n| n != b).unwrap_or(false);
        let want = (b - a) as usize + if jump { XFADE } else { 0 };
        dec.seek_exact(ms(a))?;
        let mut seg: Vec<f32> = Vec::with_capacity(want.min(1 << 22) * ch);
        let mut head_done = tail.is_none();
        let mut pos = 0usize; // trames de la plage déjà produites
        loop {
            if pos >= want {
                break;
            }
            if !dec.next_block(&mut buf)? {
                break;
            }
            let frames = buf.len() / ch;
            let take = frames.min(want - pos);
            let chunk = &mut buf[..take * ch];
            // Fondu enchaîné au début de la plage.
            if !head_done {
                if let Some(t) = &tail {
                    for j in 0..take {
                        let k = pos + j;
                        if k >= XFADE {
                            head_done = true;
                            break;
                        }
                        let x = (k as f32 + 0.5) / XFADE as f32 * std::f32::consts::FRAC_PI_2;
                        for c in 0..ch {
                            let old = t.get(k * ch + c).copied().unwrap_or(0.0);
                            chunk[j * ch + c] = old * x.cos() + chunk[j * ch + c] * x.sin();
                        }
                    }
                    if pos + take >= XFADE {
                        head_done = true;
                    }
                }
            }
            // Ce qui dépasse la plage (suite gardée pour le raccord suivant).
            let in_range = ((b - a) as usize).saturating_sub(pos).min(take);
            w.write(&chunk[..in_range * ch]).map_err(|e| e.to_string())?;
            written += in_range as u64;
            if take > in_range {
                seg.extend_from_slice(&chunk[in_range * ch..]);
            }
            pos += take;
        }
        // Plage plus courte que prévu (fin de fichier) : complétée de silence.
        let short = ((b - a) as usize).saturating_sub(pos.min((b - a) as usize));
        if short > 0 {
            w.write(&vec![0.0; short * ch]).map_err(|e| e.to_string())?;
            written += short as u64;
        }
        tail = if jump { Some(seg) } else { None };
    }
    w.finish().map_err(|e| e.to_string())?;
    Ok(ms(written))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intro_et_outro_allongees_sans_raccord_audible() {
        let dir = tempfile::tempdir().unwrap();
        let samples = crate::prep::structure::tests::club_track();
        let src = crate::testdata::write(dir.path(), "club.wav", &crate::testdata::wav_from_samples(&samples));
        let st = crate::prep::structure::analyze(&src, Some(128.0)).unwrap();
        let plan = EditPlan { intro_bars: 16, outro_bars: 8, loop_bars: 8 };
        let dst = dir.path().join("edit.wav");
        let ms = render(&src, &dst, &st, &plan).unwrap();
        // Durée : l'original + 24 mesures.
        let orig_ms = samples.len() as f64 / 2.0 / 44.1;
        assert!((ms - (orig_ms + 24.0 * st.bar_ms())).abs() < 1.0, "{ms} {orig_ms}");
        // Relu : pas de saut brutal (clic) plus grand que ceux du son d'origine.
        let mut d = Decoder::open(&dst).unwrap();
        let (mut all, mut buf) = (Vec::new(), Vec::new());
        while d.next_block(&mut buf).unwrap() {
            all.extend(buf.iter().step_by(2));
        }
        let orig: Vec<f32> = samples.iter().step_by(2).map(|v| *v as f32 / 32768.0).collect();
        let max_step = |v: &[f32]| v.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step(&all) <= max_step(&orig) * 1.05 + 1e-3, "{} {}", max_step(&all), max_step(&orig));
        // Après les boucles d'intro, le morceau reprend exactement.
        let first = (st.first_beat_ms * 44.1).round() as usize;
        let lp = (8.0 * st.bar_ms() * 44.1).round() as usize;
        let k = first + 2 * lp + 1000;
        assert!((all[k] - orig[first + 1000]).abs() < 1e-3);
        // L'original n'a pas bougé.
        assert_eq!(std::fs::read(&src).unwrap(), crate::testdata::wav_from_samples(&samples));
    }
}
