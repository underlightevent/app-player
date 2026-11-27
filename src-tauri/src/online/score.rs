//! Score de confiance (0 à 100) d'un candidat, avec son détail lisible
//! (« empreinte 0,96 · titre identique · durée +1 s »).
//!
//! Règles de prudence :
//! - sans empreinte AcoustID, le score ne peut pas dépasser 75 : un morceau trouvé seulement
//!   par son nom n'est jamais appliqué automatiquement ;
//! - deux versions différentes (Extended Mix et Radio Edit) font tomber le score sous 60 :
//!   elles ne sont jamais confondues.

use super::model::{Candidate, Query};
use crate::tags::text::comparable;

/// Ressemblance de deux textes après normalisation (0 à 1).
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (comparable(a), comparable(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    strsim::jaro_winkler(&a, &b)
}

/// Versions équivalentes : « Original Mix » et « Original » ; sinon identiques après normalisation.
pub fn same_version(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let c = comparable(s);
        match c.as_str() {
            "original" | "original mix" | "original version" => "original mix".to_string(),
            "extended" | "extended mix" | "extended version" => "extended mix".to_string(),
            "radio" | "radio edit" | "radio version" | "radio mix" => "radio edit".to_string(),
            "club" | "club mix" => "club mix".to_string(),
            _ => c,
        }
    };
    norm(a) == norm(b)
}

fn fmt(locale_fr: bool, v: f64) -> String {
    let s = format!("{v:.2}");
    if locale_fr {
        s.replace('.', ",")
    } else {
        s
    }
}

/// Calcule le score et son détail (en français ; l'interface traduit les mots-clés si besoin).
pub fn score(q: &Query, c: &mut Candidate) {
    let mut total: f64 = 0.0;
    let mut detail: Vec<String> = Vec::new();

    // Empreinte : jusqu'à 45 points.
    match c.fingerprint_score {
        Some(fp) => {
            total += 45.0 * ((fp - 0.5) / 0.45).clamp(0.0, 1.0);
            detail.push(format!("empreinte {}", fmt(true, fp)));
        }
        None => {
            // Recherche par le nom seulement : une correspondance parfaite atteint « À valider »
            // (60 à 75), jamais l'application automatique.
            total += 15.0;
            detail.push("sans empreinte".into());
        }
    }

    // Titre : jusqu'à 20 points.
    match (&q.title, &c.title) {
        (Some(a), Some(b)) => {
            let s = similarity(a, b);
            total += 20.0 * s;
            detail.push(if s >= 0.999 { "titre identique".into() } else { format!("titre {}%", (s * 100.0).round()) });
        }
        // Morceau sans titre : on s'en remet à l'empreinte.
        (None, Some(_)) if c.fingerprint_score.is_some() => total += 12.0,
        _ => {}
    }

    // Artiste principal : jusqu'à 15 points.
    match (q.artists.first(), c.artists.first()) {
        (Some(a), Some(_)) => {
            let s = c.artists.iter().map(|b| similarity(a, b)).fold(0.0, f64::max);
            total += 15.0 * s;
            detail.push(if s >= 0.999 { "artiste identique".into() } else { format!("artiste {}%", (s * 100.0).round()) });
        }
        (None, Some(_)) if c.fingerprint_score.is_some() => total += 9.0,
        _ => {}
    }

    // Version : +10 si identique, forte pénalité si différente.
    match (&q.version, &c.version) {
        (Some(a), Some(b)) if same_version(a, b) => {
            total += 10.0;
            detail.push("version identique".into());
        }
        (Some(_), Some(b)) => {
            total -= 60.0;
            detail.push(format!("autre version ({b})"));
        }
        (None, None) => total += 6.0,
        (None, Some(b)) => {
            total += 3.0;
            detail.push(format!("version proposée : {b}"));
        }
        (Some(_), None) => total += 2.0,
    }

    // Durée : +10 si l'écart est de 3 s au plus, pénalité au-delà.
    if let (Some(a), Some(b)) = (q.duration_ms, c.duration_ms) {
        let diff = (a as i64 - b as i64) as f64 / 1000.0;
        if diff.abs() <= 3.0 {
            total += 10.0;
        } else {
            total -= ((diff.abs() - 3.0) * 2.0).min(40.0);
        }
        let sign = if diff >= 0.0 { "-" } else { "+" };
        detail.push(format!("durée {sign}{} s", diff.abs().round()));
    }

    // Accord entre plusieurs sources : +5.
    if !c.confirmed_by.is_empty() {
        total += 5.0;
        detail.push(format!("confirmé par {}", c.confirmed_by.join(", ")));
    }

    if c.fingerprint_score.is_none() {
        total = total.min(75.0);
    }
    c.score = total.round().clamp(0.0, 100.0) as u8;
    c.score_detail = detail;
}

/// Décision selon les seuils réglables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Auto,
    Review,
    Unidentified,
}

pub fn decide(score: u8, auto: u8, review: u8) -> Decision {
    if score >= auto {
        Decision::Auto
    } else if score >= review {
        Decision::Review
    } else {
        Decision::Unidentified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(title: &str, version: Option<&str>, dur: u64) -> Query {
        Query {
            title: Some(title.into()),
            version: version.map(String::from),
            artists: vec!["Artiste A".into()],
            duration_ms: Some(dur),
            ..Default::default()
        }
    }

    fn cand(title: &str, version: Option<&str>, dur: u64, fp: Option<f64>) -> Candidate {
        Candidate {
            title: Some(title.into()),
            version: version.map(String::from),
            artists: vec!["Artiste A".into()],
            duration_ms: Some(dur),
            fingerprint_score: fp,
            ..Default::default()
        }
    }

    #[test]
    fn empreinte_et_tout_identique_application_automatique() {
        let mut c = cand("Titre", Some("Extended Mix"), 400_000, Some(0.96));
        score(&q("Titre", Some("Extended Mix"), 401_000), &mut c);
        assert!(c.score >= 90, "{} {:?}", c.score, c.score_detail);
        assert_eq!(decide(c.score, 90, 60), Decision::Auto);
        assert!(c.score_detail.join(" · ").contains("empreinte 0,96"));
        assert!(c.score_detail.iter().any(|d| d == "durée -1 s"));
    }

    #[test]
    fn original_et_extended_jamais_confondues() {
        // Même avec une excellente empreinte (les deux versions se ressemblent au début).
        let mut c = cand("Titre", Some("Extended Mix"), 400_000, Some(0.99));
        score(&q("Titre", Some("Original Mix"), 400_000), &mut c);
        assert!(c.score < 60, "{} {:?}", c.score, c.score_detail);
        assert_eq!(decide(c.score, 90, 60), Decision::Unidentified);
    }

    #[test]
    fn sans_empreinte_jamais_automatique() {
        let mut c = cand("Titre", Some("Extended Mix"), 400_000, None);
        score(&q("Titre", Some("Extended Mix"), 400_000), &mut c);
        assert!(c.score <= 75);
        assert_eq!(decide(c.score, 90, 60), Decision::Review, "{}", c.score);
    }

    #[test]
    fn duree_tres_differente_penalisee() {
        let mut a = cand("Titre", None, 400_000, Some(0.95));
        let mut b = cand("Titre", None, 200_000, Some(0.95));
        score(&q("Titre", None, 400_000), &mut a);
        score(&q("Titre", None, 400_000), &mut b);
        assert!(a.score >= 90);
        assert!(b.score < 90, "écart de 200 s : {}", b.score);
    }

    #[test]
    fn versions_equivalentes() {
        assert!(same_version("Original Mix", "original"));
        assert!(same_version("Extended Mix", "Extended Version"));
        assert!(!same_version("Extended Mix", "Radio Edit"));
        assert!(!same_version("Original Mix", "DJ X Remix"));
    }
}
