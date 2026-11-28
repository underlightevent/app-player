//! Morceaux similaires et générateur de set (phase 8).
//!
//! Chaque morceau est décrit par : tempo, tonalité (roue Camelot), énergie (1 à 10, phase 6),
//! genre et profil spectral (« couleur » du son, analyse de structure). Une valeur manquante
//! compte comme neutre : un morceau pas encore analysé reste proposable.

use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::djtools::{bpm_diff, key_compat};
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct Profile {
    pub id: i64,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub energy: Option<i64>,
    pub genre: Option<String>,
    pub duration_ms: i64,
    pub timbre: Option<Vec<f32>>,
}

/// Profils de tous les morceaux lisibles (ou de `ids`).
pub fn profiles(conn: &Connection, ids: Option<&[i64]>) -> Result<Vec<Profile>, AppError> {
    let mut st = conn.prepare(
        "SELECT t.id, coalesce(nullif(t.bpm, 0), a.bpm), coalesce(nullif(t.musical_key, ''), a.musical_key), a.energy, t.genre,
                coalesce(t.duration_ms, 0), s.data
         FROM tracks t LEFT JOIN analysis a ON a.track_id = t.id LEFT JOIN structure s ON s.track_id = t.id
         WHERE t.status = 'ok'",
    )?;
    let all = st.query_map([], |r| {
        let data: Option<String> = r.get(6)?;
        Ok(Profile {
            id: r.get(0)?,
            bpm: r.get(1)?,
            key: r.get(2)?,
            energy: r.get(3)?,
            genre: r.get(4)?,
            duration_ms: r.get(5)?,
            timbre: data.and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok()).and_then(|v| serde_json::from_value(v["timbre"].clone()).ok()),
        })
    })?;
    let mut out: Vec<Profile> = all.collect::<Result<_, _>>()?;
    if let Some(ids) = ids {
        let set: HashSet<i64> = ids.iter().copied().collect();
        out.retain(|p| set.contains(&p.id));
    }
    Ok(out)
}

fn key_score(a: Option<&str>, b: Option<&str>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => match key_compat(a, b) {
            "same" => 1.0,
            "adjacent" | "relative" => 0.85,
            "energy" => 0.6,
            "unknown" => 0.5,
            _ => 0.1,
        },
        _ => 0.5,
    }
}

fn bpm_score(a: Option<f64>, b: Option<f64>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) => bpm_diff(a, b).map(|d| (1.0 - d.abs() / 8.0).max(0.0)).unwrap_or(0.5),
        _ => 0.5,
    }
}

fn timbre_score(a: Option<&Vec<f32>>, b: Option<&Vec<f32>>) -> f64 {
    match (a, b) {
        (Some(a), Some(b)) if a.len() == b.len() && !a.is_empty() => {
            let d = (a.iter().zip(b).map(|(x, y)| ((x - y) as f64).powi(2)).sum::<f64>() / a.len() as f64).sqrt();
            (-d / 6.0).exp()
        }
        _ => 0.5,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Similar {
    pub id: i64,
    /// 0 à 100.
    pub score: i64,
    pub key_compat: Option<String>,
    pub bpm_diff: Option<f64>,
    /// Ressemblance du son seul (0 à 100), si les deux morceaux sont analysés.
    pub sound: Option<i64>,
}

/// Score de ressemblance (0 à 1) de `c` par rapport à `r`.
fn score(r: &Profile, c: &Profile) -> f64 {
    let energy = match (r.energy, c.energy) {
        (Some(a), Some(b)) => 1.0 - ((a - b).abs() as f64 / 5.0).min(1.0),
        _ => 0.5,
    };
    let genre = match (&r.genre, &c.genre) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => if a.to_lowercase() == b.to_lowercase() { 1.0 } else { 0.3 },
        _ => 0.5,
    };
    0.30 * bpm_score(r.bpm, c.bpm) + 0.20 * key_score(r.key.as_deref(), c.key.as_deref()) + 0.15 * energy + 0.10 * genre + 0.25 * timbre_score(r.timbre.as_ref(), c.timbre.as_ref())
}

/// Les `limit` morceaux les plus proches de `id`.
pub fn similar(conn: &Connection, id: i64, limit: usize) -> Result<Vec<Similar>, AppError> {
    let all = profiles(conn, None)?;
    let Some(r) = all.iter().find(|p| p.id == id).cloned() else { return Ok(Vec::new()) };
    let mut v: Vec<(f64, &Profile)> = all.iter().filter(|p| p.id != id).map(|p| (score(&r, p), p)).collect();
    v.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(v.into_iter()
        .take(limit)
        .map(|(s, p)| Similar {
            id: p.id,
            score: (s * 100.0).round() as i64,
            key_compat: match (&r.key, &p.key) {
                (Some(a), Some(b)) => Some(key_compat(a, b).to_string()),
                _ => None,
            },
            bpm_diff: match (r.bpm, p.bpm) {
                (Some(a), Some(b)) => bpm_diff(a, b).map(|d| (d * 10.0).round() / 10.0),
                _ => None,
            },
            sound: match (&r.timbre, &p.timbre) {
                (Some(_), Some(_)) => Some((timbre_score(r.timbre.as_ref(), p.timbre.as_ref()) * 100.0).round() as i64),
                _ => None,
            },
        })
        .collect())
}

// =====================================================================
// Générateur de set
// =====================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPlan {
    pub minutes: u32,
    /// « rise », « plateau », « wave », « fall »
    pub curve: String,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    /// Morceaux parmi lesquels choisir (bibliothèque affichée, playlist, étiquette…).
    pub pool: Vec<i64>,
    /// Premier morceau imposé.
    pub start: Option<i64>,
}

/// Énergie visée (1 à 10) à la position `t` (0 = début, 1 = fin).
pub fn target_energy(curve: &str, t: f64) -> f64 {
    match curve {
        "rise" => 3.0 + 6.0 * t,
        "fall" => 8.5 - 5.5 * t,
        "wave" => 5.5 + 2.5 * (2.0 * std::f64::consts::PI * 1.5 * t - std::f64::consts::FRAC_PI_2).sin(),
        _ => if t < 0.15 { 4.0 + 20.0 * t } else { 7.0 },
    }
}

/// Recouvrement compté entre deux morceaux mixés (le DJ ne joue pas chaque morceau en entier).
const OVERLAP_MS: i64 = 45_000;

/// Ordre de morceaux proposé : à chaque étape, le meilleur suivant (tonalité compatible,
/// tempo proche, énergie proche de la courbe), jusqu'à la durée voulue.
pub fn generate(conn: &Connection, plan: &SetPlan) -> Result<Vec<i64>, AppError> {
    let pool = profiles(conn, Some(&plan.pool))?;
    let in_range = |p: &Profile| match p.bpm {
        Some(b) => plan.bpm_min.map(|m| b >= m - 0.01).unwrap_or(true) && plan.bpm_max.map(|m| b <= m + 0.01).unwrap_or(true),
        None => plan.bpm_min.is_none() && plan.bpm_max.is_none(),
    };
    let cands: Vec<&Profile> = pool.iter().filter(|p| in_range(p) && p.duration_ms > 60_000).collect();
    if cands.is_empty() {
        return Ok(Vec::new());
    }
    let goal = plan.minutes as i64 * 60_000;
    // Durée moyenne d'un morceau joué, pour situer chaque étape sur la courbe.
    let avg = (cands.iter().map(|p| p.duration_ms).sum::<i64>() / cands.len() as i64 - OVERLAP_MS).max(60_000);
    let energy_fit = |p: &Profile, t: f64| match p.energy {
        Some(e) => 1.0 - ((e as f64 - target_energy(&plan.curve, t)).abs() / 5.0).min(1.0),
        None => 0.4,
    };
    let mut used: HashSet<i64> = HashSet::new();
    let mut order: Vec<i64> = Vec::new();
    let first = match plan.start.and_then(|s| cands.iter().find(|p| p.id == s)) {
        Some(p) => *p,
        None => *cands.iter().max_by(|a, b| energy_fit(a, 0.0).total_cmp(&energy_fit(b, 0.0)).then(b.id.cmp(&a.id))).unwrap(),
    };
    let mut cur = first;
    let mut elapsed = first.duration_ms - OVERLAP_MS;
    used.insert(first.id);
    order.push(first.id);
    while elapsed < goal {
        let t = ((elapsed + avg / 2) as f64 / goal as f64).min(1.0);
        let next = cands
            .iter()
            .filter(|p| !used.contains(&p.id))
            .map(|p| {
                let s = 0.35 * key_score(cur.key.as_deref(), p.key.as_deref())
                    + 0.30 * bpm_score(cur.bpm, p.bpm)
                    + 0.30 * energy_fit(p, t)
                    + 0.05 * timbre_score(cur.timbre.as_ref(), p.timbre.as_ref());
                (s, *p)
            })
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.id.cmp(&a.1.id)));
        let Some((_, p)) = next else { break };
        used.insert(p.id);
        order.push(p.id);
        elapsed += p.duration_ms - OVERLAP_MS;
        cur = p;
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: i64, bpm: f64, key: &str, energy: i64) -> Profile {
        Profile { id, bpm: Some(bpm), key: Some(key.into()), energy: Some(energy), genre: Some("House".into()), duration_ms: 300_000, timbre: None }
    }

    #[test]
    fn courbe_et_compatibilites() {
        assert!(target_energy("rise", 0.0) < target_energy("rise", 1.0));
        assert!(target_energy("fall", 0.0) > target_energy("fall", 1.0));
        // Morceau proche (même tonalité, tempo à 1 %) devant un morceau éloigné.
        let r = p(1, 124.0, "Am", 6);
        assert!(score(&r, &p(2, 125.0, "Am", 6)) > score(&r, &p(3, 140.0, "F#", 2)));
        // Demi-tempo reconnu.
        assert!(bpm_score(Some(87.0), Some(174.0)) > 0.99);
    }
}
