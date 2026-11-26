//! AcoustID : reconnaissance d'un morceau par son empreinte audio.
//! 3 requêtes par seconde au plus (l'application en fait 2). Clé d'application obligatoire,
//! clé utilisateur obligatoire à partir du 1er janvier 2027 (docs/SOURCES.md).

use serde_json::Value;

use super::http::{encode, Http, NetError};

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// Score de l'empreinte (0 à 1).
    pub score: f64,
    pub recording_ids: Vec<String>,
}

pub fn parse(v: &Value) -> Result<Vec<Match>, NetError> {
    if v.get("status").and_then(|s| s.as_str()) != Some("ok") {
        let msg = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("erreur inconnue");
        return Err(NetError::Bad("acoustid".into(), msg.to_string()));
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
    let mut url = format!(
        "https://api.acoustid.org/v2/lookup?format=json&meta=recordingids&client={}&duration={duration_s}&fingerprint={}",
        encode(client_key),
        encode(fingerprint)
    );
    if let Some(u) = user_key.filter(|u| !u.trim().is_empty()) {
        url.push_str(&format!("&user={}", encode(u)));
    }
    parse(&http.get_json("acoustid", &url, &[], true)?)
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

    #[test]
    fn erreur_cle() {
        let v: Value = serde_json::from_str(r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#).unwrap();
        assert!(matches!(parse(&v), Err(NetError::Bad(_, m)) if m.contains("invalid")));
    }
}
