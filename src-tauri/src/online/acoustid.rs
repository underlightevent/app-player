//! AcoustID : reconnaissance d'un morceau par son empreinte audio.
//! 3 requêtes par seconde au plus (l'application en fait 2). Clé d'application obligatoire,
//! clé utilisateur obligatoire à partir du 1er janvier 2027 (docs/SOURCES.md).
//! Les recherches sont envoyées en POST, comme le recommande AcoustID pour les longues empreintes.

use serde_json::Value;

use super::http::{Http, NetError};

const URL: &str = "https://api.acoustid.org/v2/lookup";

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// Score de l'empreinte (0 à 1).
    pub score: f64,
    pub recording_ids: Vec<String>,
}

/// Cause d'un refus d'AcoustID, d'après son code d'erreur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Code 4 : clé d'application invalide.
    ClientKey,
    /// Code 6 : clé utilisateur invalide.
    UserKey,
    /// Code 3 : empreinte refusée.
    Fingerprint,
    /// Autre refus, avec le message d'AcoustID.
    Other(String),
}

impl Problem {
    pub fn code(&self) -> String {
        match self {
            Problem::ClientKey => "invalid_client_key".into(),
            Problem::UserKey => "invalid_user_key".into(),
            Problem::Fingerprint => "invalid_fingerprint".into(),
            Problem::Other(m) => m.clone(),
        }
    }
}

fn problem_from_json(v: &Value) -> Option<Problem> {
    if v.get("status").and_then(|s| s.as_str()) == Some("ok") {
        return None;
    }
    let err = v.get("error");
    let code = err.and_then(|e| e.get("code")).and_then(|c| c.as_i64()).unwrap_or(0);
    let msg = err.and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("erreur inconnue").to_string();
    Some(match code {
        4 => Problem::ClientKey,
        6 => Problem::UserKey,
        3 => Problem::Fingerprint,
        _ => Problem::Other(msg),
    })
}

/// Explique une erreur réseau d'AcoustID (réponse 400 avec un message JSON).
pub fn problem(e: &NetError) -> Option<Problem> {
    match e {
        NetError::Rejected { body, .. } => serde_json::from_str::<Value>(body).ok().and_then(|v| problem_from_json(&v)),
        NetError::Bad(_, m) if m.starts_with("acoustid:") => Some(Problem::Other(m.trim_start_matches("acoustid:").to_string())),
        _ => None,
    }
}

pub fn parse(v: &Value) -> Result<Vec<Match>, NetError> {
    if let Some(p) = problem_from_json(v) {
        return Err(NetError::Bad("acoustid".into(), format!("acoustid:{}", p.code())));
    }
    let mut out: Vec<Match> = v
        .get("results")
        .and_then(|r| r.as_array())
        .map(|a| {
            a.iter()
                .map(|r| Match {
                    score: r.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0),
                    recording_ids: r
                        .get("recordings")
                        .and_then(|x| x.as_array())
                        .map(|x| x.iter().filter_map(|rec| rec.get("id").and_then(|i| i.as_str()).map(String::from)).collect())
                        .unwrap_or_default(),
                })
                .filter(|m| !m.recording_ids.is_empty())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

pub fn lookup(http: &Http, client_key: &str, user_key: Option<&str>, fingerprint: &str, duration_s: u32) -> Result<Vec<Match>, NetError> {
    let duration = duration_s.to_string();
    let mut form: Vec<(&str, &str)> = vec![
        ("format", "json"),
        ("meta", "recordingids"),
        ("client", client_key.trim()),
        ("duration", &duration),
        ("fingerprint", fingerprint),
    ];
    if let Some(u) = user_key.map(str::trim).filter(|u| !u.is_empty()) {
        form.push(("user", u));
    }
    match http.post_form_json("acoustid", URL, &form, true) {
        Ok(v) => parse(&v),
        Err(e) => {
            // Réponse 400 : on traduit le code d'erreur d'AcoustID.
            if let Some(p) = problem(&e) {
                return Err(NetError::Bad("acoustid".into(), format!("acoustid:{}", p.code())));
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_enregistree() {
        let v: Value = serde_json::from_str(
            r#"{"status":"ok","results":[
                {"id":"a","score":0.61,"recordings":[{"id":"rec-faible"}]},
                {"id":"b","score":0.97,"recordings":[{"id":"rec-1"},{"id":"rec-2"}]},
                {"id":"c","score":0.99}]}"#,
        )
        .unwrap();
        let m = parse(&v).unwrap();
        assert_eq!(m.len(), 2, "résultat sans enregistrement ignoré");
        assert_eq!(m[0].score, 0.97);
        assert_eq!(m[0].recording_ids, ["rec-1", "rec-2"]);
    }

    /// Essai réel, lancé à la main seulement : une clé volontairement fausse doit être reconnue comme telle.
    #[test]
    #[ignore]
    fn vraie_cle_invalide() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, path) = crate::db::open(dir.path()).unwrap();
        let http = Http::new(path);
        let fp = crate::online::fingerprint::synthetic();
        let r = lookup(&http, "cle-volontairement-fausse", None, &fp.encoded, fp.duration_s);
        eprintln!("{r:?}");
        assert!(matches!(r, Err(NetError::Bad(_, m)) if m == "acoustid:invalid_client_key"));
    }

    #[test]
    fn codes_d_erreur() {
        let rej = |body: &str| NetError::Rejected { service: "acoustid".into(), status: 400, body: body.into() };
        assert_eq!(problem(&rej(r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#)), Some(Problem::ClientKey));
        assert_eq!(problem(&rej(r#"{"status":"error","error":{"code":6,"message":"invalid user API key"}}"#)), Some(Problem::UserKey));
        assert_eq!(problem(&rej(r#"{"status":"error","error":{"code":3,"message":"invalid fingerprint"}}"#)), Some(Problem::Fingerprint));
        assert_eq!(problem(&rej("pas du json")), None);
        let v: Value = serde_json::from_str(r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#).unwrap();
        assert!(matches!(parse(&v), Err(NetError::Bad(_, m)) if m == "acoustid:invalid_client_key"));
    }
}
