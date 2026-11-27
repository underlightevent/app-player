//! Vérification des mises à jour : demande à GitHub le numéro de la dernière version
//! publiée. Rien n'est envoyé sur l'utilisateur ni sur sa bibliothèque. Désactivable
//! (Réglages › Mises à jour). L'installation reste un geste de l'utilisateur : la page
//! de téléchargement s'ouvre dans son navigateur (voir docs/DECISIONS.md, D-406).

use std::time::Duration;

use serde::Serialize;

use crate::error::AppError;

pub const RELEASES_API: &str = "https://api.github.com/repos/underlightevent/app-player/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/underlightevent/app-player/releases/latest";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    pub url: String,
    pub notes: Option<String>,
}

/// « v0.10.2 » → (0, 10, 2). Les suffixes (« -beta ») sont ignorés.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub fn check() -> Result<UpdateInfo, AppError> {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut resp = agent
        .get(RELEASES_API)
        .header("User-Agent", format!("dj-library-manager/{}", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| AppError::Network(e.to_string()))?;
    if resp.status() != 200 {
        return Err(AppError::Network(format!("GitHub a répondu {}", resp.status())));
    }
    let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| AppError::Network(e.to_string()))?;
    let latest = v["tag_name"].as_str().unwrap_or_default().to_string();
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(UpdateInfo {
        newer: is_newer(&latest, &current),
        url: v["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_string(),
        notes: v["body"].as_str().map(|s| s.chars().take(2000).collect()),
        latest: latest.trim_start_matches('v').to_string(),
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaison_des_versions() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert!(is_newer("v0.5.0", "0.4.0"));
        assert!(is_newer("v0.10.0", "0.9.9"), "comparaison numérique, pas alphabétique");
        assert!(!is_newer("v0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.0", "0.4.0"));
        assert!(!is_newer("", "0.4.0"));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
    }

    /// Appel réel à GitHub (hors CI) : `cargo test vraie_verification -- --ignored`
    #[test]
    #[ignore]
    fn vraie_verification() {
        let u = check().unwrap();
        assert!(!u.latest.is_empty());
    }
}
